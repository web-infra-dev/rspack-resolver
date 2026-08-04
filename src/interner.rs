//! A refcounted, sharded global string interner that reclaims its own entries.
//!
//! Exists because the two obvious off-the-shelf choices each fail one half of
//! what [`crate::UstrPath`] needs:
//!
//! - `ustr` never frees, so a dev server's RSS climbs monotonically across
//!   rebuilds.
//! - `internment::ArcIntern` frees, but its container hashes the string with
//!   `ahash` behind a `DashMap`, so it re-hashes a string whose hash the caller
//!   already computed — measured at 2.5 full-string hashes per intern, and
//!   1004 Ir per intern against `ustr`'s 161.
//!
//! This one takes the hash as a parameter and keeps it in the entry header, so
//! interning hashes the string exactly once and [`Interned::hash`] is a load
//! rather than a rehash. That header is also why entries are hand-allocated
//! instead of being `Arc<str>`: an `Arc<str>` is a fat pointer with nowhere to
//! put the hash, which would push every stored path from 8 bytes to 24.
//!
//! # Reclamation
//!
//! **The table owns a reference.** An entry's count is one for the table plus
//! one per live handle, so dropping a handle can never reach zero and never has
//! to free anything — [`Interned::drop`] is a bare decrement, with no lock, no
//! table lookup, and no race to lose.
//!
//! Freeing happens in [`Shard::sweep`], which removes every entry whose count
//! is back down to one. Sweeps are triggered by insertions, at a threshold that
//! scales with the shard, so each insertion amortizes to a constant number of
//! checks however large the table grows. Memory therefore comes back on a
//! bounded delay rather than instantly, which is what a dev server needs: the
//! table cannot grow without bound when paths are transient, and it holds
//! exactly the live set when they are retained.
//!
//! # Soundness
//!
//! Sweeping on `count == 1` rests on one invariant:
//!
//! > Every operation that takes a count from 1 to 2 happens under that shard's
//! > lock.
//!
//! - The only way to obtain a handle to a table entry is [`Interner::intern`],
//!   which holds at least the read lock while it clones.
//! - A sweep holds the **write** lock, so no `intern` can be mid-clone.
//! - [`Interned::clone`] takes no lock, but its caller already holds a handle,
//!   so the count it raises is at least 2 — never a 1.
//!
//! A count of 1 observed under the write lock therefore means the table is the
//! only owner and no one else can reach the entry. `Arc` reasons about its own
//! count the same way; the relaxed load is ordered by the lock, since a clone
//! that released the read lock happens-before the sweeper's write lock.

use std::{
  alloc::{self, Layout},
  ptr::NonNull,
  slice, str,
  sync::{
    atomic::{AtomicUsize, Ordering},
    PoisonError, RwLock,
  },
};

use hashbrown::HashTable;

/// Interner entry: header followed by the string bytes in the same allocation.
///
/// One allocation rather than a header plus a `Box<str>` because interning is
/// allocation-bound, and doubling the allocation count per entry is directly
/// visible in the resolver benchmarks.
#[repr(C)]
struct Entry {
  /// One for the table, plus one per live [`Interned`].
  count: AtomicUsize,
  /// The caller's hash of the string. Stored so the handle stays one pointer
  /// wide and [`Interned::hash`] costs a load instead of a re-hash.
  hash: u64,
  len: usize,
  // `len` bytes of UTF-8 follow.
}

/// Offset of the string bytes within an entry allocation.
///
/// `[u8]` has alignment 1, so `Layout::extend` places it immediately after the
/// header with no padding — `Entry::layout` debug-asserts that this constant
/// and the computed offset agree. Spelling it as a constant keeps `Entry::str`,
/// which every `as_str()` goes through, down to one add.
const PAYLOAD_OFFSET: usize = std::mem::size_of::<Entry>();

impl Entry {
  /// Layout of a `len`-byte entry, plus the offset of the string bytes.
  ///
  /// `alloc` and `dealloc` must agree exactly, so both go through here.
  fn layout(len: usize) -> (Layout, usize) {
    let (layout, offset) = Layout::new::<Self>()
      .extend(Layout::array::<u8>(len).expect("interned string length fits in a Layout"))
      .expect("interner entry layout");
    debug_assert_eq!(offset, PAYLOAD_OFFSET);
    (layout.pad_to_align(), offset)
  }

  /// Allocate an entry holding `s`, with the count already at two: one for the
  /// table it is about to enter, one for the handle the caller gets back.
  fn alloc(s: &str, hash: u64) -> NonNull<Self> {
    let (layout, offset) = Self::layout(s.len());
    // SAFETY: `layout` has non-zero size — the header alone is non-empty.
    let Some(ptr) = NonNull::new(unsafe { alloc::alloc(layout) }) else {
      alloc::handle_alloc_error(layout)
    };
    // `alloc` returns memory aligned to `layout`, which starts from
    // `Layout::new::<Self>()`, so this is `Self`'s alignment by construction.
    let ptr = ptr.cast::<Self>();
    // SAFETY: `ptr` is freshly allocated for `layout`, so it is valid for
    // writes of the header, and of `s.len()` bytes at `offset`. The two ranges
    // cannot overlap `s`, which lives in the caller's memory.
    unsafe {
      ptr.as_ptr().write(Self {
        count: AtomicUsize::new(2),
        hash,
        len: s.len(),
      });
      ptr
        .as_ptr()
        .cast::<u8>()
        .add(offset)
        .copy_from_nonoverlapping(s.as_ptr(), s.len());
    }
    ptr
  }

  /// # Safety
  ///
  /// `ptr` must come from [`Entry::alloc`], must not have been deallocated,
  /// and must already be out of its shard with a count of one.
  unsafe fn dealloc(ptr: NonNull<Self>) {
    // SAFETY: the caller guarantees `ptr` is a live entry, so reading `len`
    // reproduces the layout it was allocated with.
    let (layout, _) = Self::layout(unsafe { ptr.as_ref() }.len);
    // SAFETY: same allocation and same layout as `alloc` used.
    unsafe { alloc::dealloc(ptr.as_ptr().cast::<u8>(), layout) }
  }

  /// # Safety
  ///
  /// `ptr` must point at a live entry, and the returned reference must not
  /// outlive the handle or table slot that keeps it alive.
  #[inline]
  unsafe fn str<'a>(ptr: NonNull<Self>) -> &'a str {
    // SAFETY: the caller guarantees `ptr` is live.
    let len = unsafe { ptr.as_ref() }.len;
    // SAFETY: `alloc` wrote exactly `len` bytes of a `&str` at
    // `PAYLOAD_OFFSET`, so the range is initialized, in bounds, and UTF-8.
    unsafe {
      let bytes = slice::from_raw_parts(ptr.as_ptr().cast::<u8>().add(PAYLOAD_OFFSET), len);
      str::from_utf8_unchecked(bytes)
    }
  }
}

/// `NonNull` is neither `Send` nor `Sync`, so it cannot go in a `static`
/// directly. Entries are: their bytes are immutable and `count` is atomic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EntryPtr(NonNull<Entry>);

// SAFETY: see the type's doc comment — an entry is immutable apart from its
// atomic count, and freeing one requires the shard's write lock.
unsafe impl Send for EntryPtr {}
// SAFETY: as above.
unsafe impl Sync for EntryPtr {}

/// Enough shards that a build's worth of resolver threads rarely collide,
/// matching what `ustr` uses. Must stay a power of two — [`shard_index`] slices
/// the index out of the hash's top bits.
const SHARDS: usize = 64;
const SHARD_BITS: u32 = SHARDS.trailing_zeros();
const _: () = assert!(SHARDS.is_power_of_two());

/// Smallest table a shard bothers sweeping. Below this, walking the table costs
/// more than the handful of entries it could reclaim.
const MIN_SWEEP_THRESHOLD: usize = 64;

struct Shard {
  table: HashTable<EntryPtr>,
  inserts_since_sweep: usize,
}

impl Shard {
  const fn new() -> Self {
    Self {
      table: HashTable::new(),
      inserts_since_sweep: 0,
    }
  }

  /// Sweep once insertions since the last sweep reach half the table. Scaling
  /// the threshold with the table is what keeps this amortized constant: a
  /// sweep walks `len` entries and buys `len / 2` insertions, so the per-insert
  /// cost stays flat however large the table grows.
  fn threshold(&self) -> usize {
    MIN_SWEEP_THRESHOLD.max(self.table.len() / 2)
  }

  /// Remove every entry no handle refers to, returning them for the caller to
  /// free once the lock is released.
  fn sweep(&mut self) -> Vec<EntryPtr> {
    self.inserts_since_sweep = 0;
    self
      .table
      .extract_if(|&mut EntryPtr(entry)| {
        // SAFETY: entries leave the table only here, under the write lock, so
        // everything still reachable is live.
        unsafe { entry.as_ref() }.count.load(Ordering::Acquire) == 1
      })
      .collect()
  }
}

/// A string interner. Distinct instances have distinct tables.
///
/// The crate uses one global instance; this is a type rather than a set of free
/// functions so that tests can work against an isolated interner instead of
/// racing each other through process-wide state.
pub struct Interner {
  shards: [RwLock<Shard>; SHARDS],
}

impl Interner {
  pub const fn new() -> Self {
    Self {
      shards: [const { RwLock::new(Shard::new()) }; SHARDS],
    }
  }

  /// Intern `s`, whose hash the caller has already computed.
  ///
  /// `hash` must be a deterministic function of `s` alone; two calls with the
  /// same string and different hashes would create two entries for it and break
  /// the one-entry-per-string guarantee that [`Interned::ptr_eq`] relies on. It
  /// is not otherwise constrained — [`crate::UstrPath`] deliberately passes a
  /// hash that folds equivalent Windows spellings together, which only makes
  /// those spellings share a shard and a bucket.
  pub fn intern(&self, s: &str, hash: u64) -> Interned {
    let lock = &self.shards[shard_index(hash)];

    // Hit path: a read lock, so threads interning paths that are already known
    // — the common case once a build is warm — never wait on each other.
    {
      let shard = lock.read().unwrap_or_else(PoisonError::into_inner);
      if let Some(ptr) = find(&shard.table, s, hash) {
        return ptr;
      }
    }

    // Miss path. The read lock was released above, so re-check: another thread
    // may have interned `s` in that window.
    let mut shard = lock.write().unwrap_or_else(PoisonError::into_inner);
    if let Some(ptr) = find(&shard.table, s, hash) {
      return ptr;
    }

    let ptr = Entry::alloc(s, hash);
    shard
      .table
      .insert_unique(hash, EntryPtr(ptr), |&EntryPtr(entry)| {
        // SAFETY: reachable table entries are live.
        unsafe { entry.as_ref() }.hash
      });
    shard.inserts_since_sweep += 1;
    let doomed = if shard.inserts_since_sweep > shard.threshold() {
      shard.sweep()
    } else {
      Vec::new()
    };
    drop(shard);

    // Freeing outside the lock keeps the critical section to table work.
    free_all(doomed);
    Interned { ptr }
  }

  /// Reclaim every entry no handle refers to, across all shards.
  ///
  /// Sweeps are otherwise driven by insertions, so a shard that has gone quiet
  /// keeps its dead entries indefinitely. Call this to collect them anyway.
  ///
  /// Test-gated only because nothing in the crate calls it yet. The natural
  /// caller is `Cache::clear()` — rspack runs it on every rebuild — which would
  /// turn the bounded-delay reclamation into immediate reclamation.
  #[cfg(test)]
  pub fn sweep(&self) {
    for lock in &self.shards {
      let doomed = lock.write().unwrap_or_else(PoisonError::into_inner).sweep();
      free_all(doomed);
    }
  }

  /// How many entries the interner holds. Diagnostic and test use — the count
  /// includes entries that are dead but not yet swept.
  #[cfg(test)]
  pub fn len(&self) -> usize {
    self
      .shards
      .iter()
      .map(|lock| {
        let shard = lock.read().unwrap_or_else(PoisonError::into_inner);
        let len = shard.table.len();
        drop(shard);
        len
      })
      .sum()
  }
}

impl Default for Interner {
  fn default() -> Self {
    Self::new()
  }
}

impl Drop for Interner {
  /// Free what the table still owns. Without this, dropping an `Interner`
  /// leaks every entry in it — the global one never drops, but tests build
  /// their own, and a leak checker sees them.
  ///
  /// Entries a handle still refers to are deliberately leaked instead. An
  /// [`Interned`] does not borrow from its `Interner`, so freeing those would
  /// leave the holder with a dangling pointer; outliving the interner is a bug
  /// at the call site, and leaking is the safe way to report it.
  fn drop(&mut self) {
    for lock in &mut self.shards {
      let shard = lock.get_mut().unwrap_or_else(PoisonError::into_inner);
      for EntryPtr(entry) in shard.table.drain() {
        // SAFETY: `&mut self` means no other thread can reach these entries,
        // and the count tells us whether any handle still can.
        if unsafe { entry.as_ref() }.count.load(Ordering::Acquire) == 1 {
          // SAFETY: table-only, and just drained out of the table.
          unsafe { Entry::dealloc(entry) }
        } else {
          debug_assert!(
            false,
            "an Interned outlived its Interner; the handle is now dangling"
          );
        }
      }
    }
  }
}

/// Look `s` up and take a reference to it. The caller must hold the shard lock.
#[inline]
fn find(table: &HashTable<EntryPtr>, s: &str, hash: u64) -> Option<Interned> {
  let &EntryPtr(ptr) = table.find(hash, |&EntryPtr(entry)| {
    // SAFETY: entries leave the table only under the write lock, which the
    // caller's lock excludes, so everything reachable here is live.
    unsafe { Entry::str(entry) == s }
  })?;
  // SAFETY: as above.
  unsafe { ptr.as_ref() }
    .count
    .fetch_add(1, Ordering::Relaxed);
  Some(Interned { ptr })
}

/// # Panics
///
/// Never; the entries come from a sweep, which only yields what it removed.
fn free_all(doomed: Vec<EntryPtr>) {
  for EntryPtr(entry) in doomed {
    // SAFETY: a sweep yields only entries it removed from its table under the
    // write lock, each with a count of one — the table's own reference.
    unsafe { Entry::dealloc(entry) }
  }
}

/// Sharded by the **top** bits of the hash: `hashbrown` indexes buckets with
/// the low bits, so sharding on those would give every entry in a shard the
/// same bucket index.
#[inline]
fn shard_index(hash: u64) -> usize {
  (hash >> (u64::BITS - SHARD_BITS)) as usize
}

/// The process-wide interner.
static GLOBAL: Interner = Interner::new();

/// Intern `s` into the process-wide interner. See [`Interner::intern`].
#[inline]
pub fn intern(s: &str, hash: u64) -> Interned {
  GLOBAL.intern(s, hash)
}

/// Entry count of the process-wide interner. See [`Interner::len`].
#[cfg(test)]
pub fn live_entries() -> usize {
  GLOBAL.len()
}

/// A refcounted handle to a globally interned string.
///
/// One pointer wide. Not `Copy`: the count has to be maintained.
pub struct Interned {
  ptr: NonNull<Entry>,
}

// SAFETY: `Entry` is immutable apart from `count`, which is atomic, and an
// entry is freed only under its shard's write lock (see module docs).
unsafe impl Send for Interned {}
// SAFETY: as above — sharing `&Interned` only exposes immutable string bytes.
unsafe impl Sync for Interned {}

impl Interned {
  #[inline]
  pub fn as_str(&self) -> &str {
    // SAFETY: `self` holds a reference, so the entry is live, and the returned
    // borrow is tied to `self`.
    unsafe { Entry::str(self.ptr) }
  }

  /// The hash handed to [`intern`], read back from the entry header.
  #[inline]
  pub fn hash(&self) -> u64 {
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }.hash
  }

  /// Whether both handles name the same interner entry.
  ///
  /// Interning guarantees one entry per distinct string, so this settles
  /// equality for byte-identical strings.
  #[inline]
  pub fn ptr_eq(&self, other: &Self) -> bool {
    self.ptr == other.ptr
  }

  /// One for the table plus one per live handle. Test and diagnostic use — the
  /// count is a snapshot and can change concurrently.
  #[cfg(test)]
  pub(crate) fn refcount(&self) -> usize {
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }.count.load(Ordering::Acquire)
  }
}

impl Clone for Interned {
  #[inline]
  fn clone(&self) -> Self {
    // No lock: the caller holds a handle, so this raises a count of at least
    // two, never the 1 -> 2 transition a sweep must not miss.
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }
      .count
      .fetch_add(1, Ordering::Relaxed);
    Self { ptr: self.ptr }
  }
}

impl Drop for Interned {
  #[inline]
  fn drop(&mut self) {
    // The table's own reference keeps this from reaching zero, so there is
    // nothing to free and nothing to lock. `Release` pairs with the acquire the
    // sweeper gets from the shard's write lock.
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }
      .count
      .fetch_sub(1, Ordering::Release);
  }
}

#[cfg(test)]
mod tests {
  use std::{sync::Arc, thread};

  use super::{Interned, Interner};

  fn h(s: &str) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(s.as_bytes());
    hasher.finish()
  }

  /// Tests use their own interner rather than the global one, so they cannot
  /// perturb each other's counts through process-wide state.
  fn get(interner: &Interner, s: &str) -> Interned {
    interner.intern(s, h(s))
  }

  #[test]
  fn equal_strings_share_one_entry() {
    let interner = Interner::new();
    let a = get(&interner, "/a/b/c.js");
    let b = get(&interner, "/a/b/c.js");
    assert!(a.ptr_eq(&b));
    assert_eq!(a.as_str(), "/a/b/c.js");
    assert_eq!(interner.len(), 1);
  }

  #[test]
  fn different_strings_get_different_entries() {
    let interner = Interner::new();
    assert!(!get(&interner, "/a.js").ptr_eq(&get(&interner, "/b.js")));
    assert_eq!(interner.len(), 2);
  }

  #[test]
  fn the_empty_string_round_trips() {
    // Zero-length payload is the one case where the entry is header-only.
    let interner = Interner::new();
    let a = get(&interner, "");
    assert_eq!(a.as_str(), "");
    assert!(a.ptr_eq(&get(&interner, "")));
  }

  #[test]
  fn hash_is_returned_verbatim() {
    // The interner must not re-derive the hash: `UstrPath` relies on getting
    // back exactly what it computed, including a Windows-folded value that
    // does not match the stored bytes.
    let interner = Interner::new();
    assert_eq!(
      interner.intern("/x.js", 0xDEAD_BEEF_1234_5678).hash(),
      0xDEAD_BEEF_1234_5678
    );
  }

  #[test]
  fn colliding_hashes_stay_distinct_entries() {
    // Windows folding hands equal hashes to different strings on purpose, so
    // the table must disambiguate by bytes rather than trusting the hash.
    let interner = Interner::new();
    let a = interner.intern("/one", 0x5555_5555_5555_5555);
    let b = interner.intern("/two", 0x5555_5555_5555_5555);
    assert!(!a.ptr_eq(&b));
    assert_eq!(a.as_str(), "/one");
    assert_eq!(b.as_str(), "/two");
  }

  #[test]
  fn refcount_counts_the_table_plus_every_handle() {
    let interner = Interner::new();
    let a = get(&interner, "/a.js");
    assert_eq!(a.refcount(), 2, "the table holds one reference of its own");
    let b = a.clone();
    assert_eq!(a.refcount(), 3);
    drop(b);
    assert_eq!(a.refcount(), 2);
  }

  #[test]
  fn sweeping_reclaims_entries_no_handle_refers_to() {
    let interner = Interner::new();
    drop(get(&interner, "/dead.js"));
    let alive = get(&interner, "/alive.js");
    assert_eq!(interner.len(), 2, "dropping a handle does not remove it");

    interner.sweep();

    assert_eq!(interner.len(), 1);
    assert_eq!(
      alive.as_str(),
      "/alive.js",
      "a held entry must survive the sweep"
    );
  }

  #[test]
  fn a_held_entry_survives_repeated_sweeps() {
    let interner = Interner::new();
    let held = get(&interner, "/held.js");
    for _ in 0..10 {
      interner.sweep();
      assert!(held.ptr_eq(&get(&interner, "/held.js")));
    }
    assert_eq!(held.as_str(), "/held.js");
  }

  #[test]
  fn one_shot_strings_do_not_grow_the_table_without_bound() {
    // The property the insert-driven sweep exists for: a build that interns a
    // hundred thousand paths and immediately discards them must not accumulate
    // them. The bound is roughly SHARDS * MIN_SWEEP_THRESHOLD.
    let interner = Interner::new();
    for i in 0..100_000 {
      drop(get(&interner, &format!("/transient/{i}.js")));
    }
    let len = interner.len();
    assert!(len < 8_192, "table grew to {len}, which is not bounded");
  }

  #[test]
  fn concurrent_intern_and_drop_is_sound() {
    // Threads race interning and discarding a small key space while sweeps fire
    // underneath them, driven by the one-shot strings. A sweep that freed an
    // entry another thread had just cloned surfaces here under ASan or Miri.
    let interner = Arc::new(Interner::new());
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
      .map(|t| {
        let interner = Arc::clone(&interner);
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
          barrier.wait();
          for i in 0..5000 {
            let shared = format!("/shared/{}", i % 4);
            let handle = get(&interner, &shared);
            assert_eq!(handle.as_str(), shared);
            let cloned = handle.clone();
            drop(handle);
            assert_eq!(cloned.as_str(), shared);

            let once = format!("/once/{t}/{i}");
            assert_eq!(get(&interner, &once).as_str(), once);
          }
        })
      })
      .collect();
    for thread in threads {
      thread.join().expect("interner race thread panicked");
    }

    interner.sweep();
    assert_eq!(
      interner.len(),
      0,
      "every handle is gone, so a full sweep must empty the table"
    );
  }
}
