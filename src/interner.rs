//! A refcounted, sharded global string interner.
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
//! interning hashes the string exactly once and [`Interned::hash`] is a load.
//!
//! # Concurrency
//!
//! Refcount changes are lock-free; only the table is locked. [`intern`] holds
//! the shard lock while it looks up and increments, and a freed entry leaves
//! the table under that same lock, so the two never overlap. Both
//! [`Interned::clone`] and [`Interned::drop`] touch the count without any lock.
//!
//! Dropping to zero is therefore a *claim* on the entry, not ownership of it.
//! Between that decrement and the shard lock, two things can happen, and the
//! drop path checks for both before freeing:
//!
//! - **Resurrection.** An `intern` of the same string finds the entry and
//!   increments it back. The count then names exactly the handles that exist,
//!   so the dropper simply walks away — hence the `count != 0` re-check under
//!   the lock.
//! - **A second claim.** The resurrected handle is itself dropped to zero, and
//!   two threads now believe they must free. Removal happens under the lock, so
//!   the loser finds the entry already absent — hence the lookup by address,
//!   which must not dereference, before anything else.
//!
//! `clone` needs neither check: its caller holds a handle, so the count it
//! increments is at least one and cannot be a resurrection.

use std::{
  alloc::{self, Layout},
  ptr::NonNull,
  slice, str,
  sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, MutexGuard, PoisonError,
  },
};

use hashbrown::HashTable;

/// Interner entry: header followed by the string bytes in the same allocation.
///
/// One allocation rather than a header plus a `Box<str>` because interning is
/// allocation-bound — a refcounted interner re-allocates every entry that was
/// freed since the last use, and doubling the allocation count per entry is
/// directly visible in the resolver benchmarks.
#[repr(C)]
struct Entry {
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

  /// Allocate an entry holding `s`, with the refcount already at one for the
  /// handle the caller is about to build.
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
        count: AtomicUsize::new(1),
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
  /// and must be unreachable — removed from the table with a zero refcount.
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
  /// outlive the handle that keeps it alive.
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

/// Enough shards that four resolver threads rarely collide, matching what
/// `ustr` uses. Must stay a power of two — [`shard`] slices the index out of
/// the hash's top bits.
const SHARDS: usize = 64;
const SHARD_BITS: u32 = SHARDS.trailing_zeros();
const _: () = assert!(SHARDS.is_power_of_two());

/// `NonNull` is neither `Send` nor `Sync`, so it cannot go in a `static`
/// directly. Entries are: their bytes are immutable and `count` is atomic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct EntryPtr(NonNull<Entry>);

// SAFETY: see the type's doc comment — an entry is immutable apart from its
// atomic refcount, and freeing one requires the shard lock.
unsafe impl Send for EntryPtr {}
// SAFETY: as above.
unsafe impl Sync for EntryPtr {}

type Shard = Mutex<HashTable<EntryPtr>>;

/// The process-wide interner.
///
/// Sharded by the **top** bits of the hash: `hashbrown` indexes buckets with
/// the low bits, so sharding on those would give every entry in a shard the
/// same bucket index.
static SHARD_TABLE: [Shard; SHARDS] = [const { Mutex::new(HashTable::new()) }; SHARDS];

fn shard(hash: u64) -> MutexGuard<'static, HashTable<EntryPtr>> {
  let index = (hash >> (u64::BITS - SHARD_BITS)) as usize;
  // A panic while holding a shard lock cannot leave the table inconsistent:
  // the only calls made under it are hash-table operations and the allocator,
  // neither of which unwinds partway through a mutation. Recovering beats
  // poisoning every future intern of that shard.
  SHARD_TABLE[index]
    .lock()
    .unwrap_or_else(PoisonError::into_inner)
}

/// A refcounted handle to a globally interned string.
///
/// One pointer wide. Not `Copy`: the refcount has to be maintained.
pub struct Interned {
  ptr: NonNull<Entry>,
}

// SAFETY: `Entry` is immutable apart from `count`, which is atomic, and every
// operation that can free it happens under the shard lock (see module docs).
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

  /// How many handles currently share this entry. Test and diagnostic use —
  /// the count is a snapshot and can change concurrently.
  #[cfg(test)]
  pub(crate) fn refcount(&self) -> usize {
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }.count.load(Ordering::Acquire)
  }
}

/// How many entries the interner currently holds, across all shards.
///
/// Diagnostic only, and inherently a snapshot: the interner is process-wide, so
/// this counts every string every thread is holding, not just the caller's.
#[cfg(test)]
pub fn live_entries() -> usize {
  SHARD_TABLE
    .iter()
    .map(|shard| {
      let table = shard.lock().unwrap_or_else(PoisonError::into_inner);
      let len = table.len();
      drop(table);
      len
    })
    .sum()
}

/// Intern `s`, whose hash the caller has already computed.
///
/// `hash` must be a deterministic function of `s` alone; two calls with the
/// same string and different hashes would create two entries for it and break
/// the one-entry-per-string guarantee that [`Interned::ptr_eq`] relies on. It
/// is not otherwise constrained — [`crate::UstrPath`] deliberately passes a
/// hash that folds equivalent Windows spellings together, which only makes
/// those spellings share a shard and a bucket.
pub fn intern(s: &str, hash: u64) -> Interned {
  let mut table = shard(hash);
  if let Some(&EntryPtr(ptr)) = table.find(hash, |&EntryPtr(entry)| {
    // SAFETY: an entry is removed from the table before it is freed, and both
    // happen under this lock, so everything reachable here is alive.
    unsafe { Entry::str(entry) == s }
  }) {
    // The old value may be 0 — a dropper claimed this entry and is waiting for
    // the lock. Resurrecting it is exactly right: it will see a non-zero count
    // and leave the entry alone. Relaxed is enough because the lock we hold
    // orders this against that check.
    // SAFETY: the entry is in the table, so it is alive.
    unsafe { ptr.as_ref() }
      .count
      .fetch_add(1, Ordering::Relaxed);
    return Interned { ptr };
  }
  let ptr = Entry::alloc(s, hash);
  table.insert_unique(hash, EntryPtr(ptr), |&EntryPtr(entry)| {
    // SAFETY: reachable table entries are alive (see above).
    unsafe { entry.as_ref() }.hash
  });
  // The handle already owns the entry's first reference, so releasing the
  // shard before building it is safe and keeps the critical section minimal.
  drop(table);
  Interned { ptr }
}

impl Clone for Interned {
  #[inline]
  fn clone(&self) -> Self {
    // No lock: `self` is a live handle, so the count is at least one both
    // before and after. See the module's concurrency notes.
    // SAFETY: `self` holds a reference, so the entry is live.
    unsafe { self.ptr.as_ref() }
      .count
      .fetch_add(1, Ordering::Relaxed);
    Self { ptr: self.ptr }
  }
}

impl Drop for Interned {
  fn drop(&mut self) {
    // Read the hash while we still own a reference. Past the decrement below,
    // `self.ptr` is only an address — dereferencing it is unsound until the
    // shard lock re-establishes that the entry is still alive.
    let hash = self.hash();
    // SAFETY: `self` still holds a reference, so the entry is live.
    if unsafe { self.ptr.as_ref() }
      .count
      .fetch_sub(1, Ordering::AcqRel)
      != 1
    {
      // The overwhelming majority of drops land here — a temporary handle
      // going away while the cache still holds the path. Keeping this path
      // lock-free is what stops the shards from serializing resolver threads.
      return;
    }
    let mut table = shard(hash);
    // Address comparison only: a concurrent dropper may already have freed
    // this entry, so it must not be dereferenced before it is found.
    let Ok(entry) = table.find_entry(hash, |&EntryPtr(candidate)| candidate == self.ptr) else {
      // Gone from the table, which only happens after another thread claimed
      // the free. Nothing left to do — and nothing left to read.
      return;
    };
    // Found under the lock, and entries are freed only after being removed
    // under this same lock, so the entry is alive again for as long as we hold
    // it.
    // SAFETY: as above.
    if unsafe { self.ptr.as_ref() }.count.load(Ordering::Acquire) != 0 {
      // Resurrected: an `intern` found this entry between our decrement and
      // our lock, and now owns it. Leaving it in place is correct — the count
      // reflects exactly the handles that exist.
      return;
    }
    entry.remove();
    drop(table);
    // SAFETY: zero refcount and out of the table, so no handle can exist and
    // no lookup can reach it.
    unsafe { Entry::dealloc(self.ptr) }
  }
}

#[cfg(test)]
mod tests {
  use std::{sync::Arc, thread};

  use super::{intern, Interned};

  /// The interner is process-wide and shared with every other test running in
  /// parallel, so assertions here use strings no other test interns and only
  /// ever check one entry's own state.
  fn h(s: &str) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = rustc_hash::FxHasher::default();
    hasher.write(s.as_bytes());
    hasher.finish()
  }

  fn get(s: &str) -> Interned {
    intern(s, h(s))
  }

  #[test]
  fn equal_strings_share_one_entry() {
    let a = get("interner::equal_strings/a/b/c.js");
    let b = get("interner::equal_strings/a/b/c.js");
    assert!(a.ptr_eq(&b));
    assert_eq!(a.as_str(), "interner::equal_strings/a/b/c.js");
  }

  #[test]
  fn different_strings_get_different_entries() {
    let a = get("interner::different/a.js");
    let b = get("interner::different/b.js");
    assert!(!a.ptr_eq(&b));
  }

  #[test]
  fn the_empty_string_round_trips() {
    // Zero-length payload is the one case where the entry is header-only.
    let a = get("");
    assert_eq!(a.as_str(), "");
    assert!(a.ptr_eq(&get("")));
  }

  #[test]
  fn hash_is_returned_verbatim() {
    // The interner must not re-derive the hash: `UstrPath` relies on getting
    // back exactly what it computed, including a Windows-folded value that
    // does not match the stored bytes.
    let interned = intern("interner::verbatim/x.js", 0xDEAD_BEEF_1234_5678);
    assert_eq!(interned.hash(), 0xDEAD_BEEF_1234_5678);
  }

  #[test]
  fn colliding_hashes_stay_distinct_entries() {
    // Windows folding hands equal hashes to different strings on purpose, so
    // the table must disambiguate by bytes rather than trusting the hash.
    let a = intern("interner::collide/one", 0x5555_5555_5555_5555);
    let b = intern("interner::collide/two", 0x5555_5555_5555_5555);
    assert!(!a.ptr_eq(&b));
    assert_eq!(a.as_str(), "interner::collide/one");
    assert_eq!(b.as_str(), "interner::collide/two");
  }

  #[test]
  fn refcount_tracks_handles() {
    let a = get("interner::refcount/a.js");
    assert_eq!(a.refcount(), 1);
    let b = a.clone();
    assert_eq!(a.refcount(), 2);
    drop(b);
    assert_eq!(a.refcount(), 1);
  }

  #[test]
  fn dropping_the_last_handle_frees_the_entry() {
    let first = get("interner::freed/a.js");
    let address = first.as_str().as_ptr();
    drop(first);

    // Re-interning after the entry was freed must produce a working handle.
    // (It may or may not reuse `address` — the allocator decides — so the
    // assertion is on the contents, not the pointer.)
    let second = get("interner::freed/a.js");
    assert_eq!(second.as_str(), "interner::freed/a.js");
    assert_eq!(
      second.refcount(),
      1,
      "the freed entry must not have lingered"
    );
    let _ = address;
  }

  #[test]
  fn concurrent_zero_crossings_are_sound() {
    // Aimed squarely at the two windows the lock-free drop path opens:
    // resurrection, and two threads both claiming the free. Every handle here
    // is created and dropped immediately over a key space of two, so threads
    // are almost always racing on an entry that is at or near zero — the state
    // a version that freed unconditionally after `fetch_sub` would corrupt.
    //
    // Reads `as_str()` after the clone specifically so a use-after-free lands
    // on the string bytes, where ASan or Miri will catch it.
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
      .map(|_| {
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
          barrier.wait();
          for i in 0..5000 {
            let key = format!("interner::zero-cross/{}", i % 2);
            let handle = get(&key);
            assert_eq!(handle.as_str(), key);
            drop(handle);
          }
        })
      })
      .collect();
    for thread in threads {
      thread.join().expect("zero-crossing thread panicked");
    }

    // Every handle above is gone, so nothing may have leaked a live entry.
    for i in 0..2 {
      let key = format!("interner::zero-cross/{i}");
      let fresh = get(&key);
      assert_eq!(
        fresh.refcount(),
        1,
        "{key} outlived every handle that referenced it"
      );
    }
  }

  #[test]
  fn concurrent_intern_and_drop_of_one_string_is_sound() {
    // Drives the race the shard lock exists for: threads repeatedly take the
    // last handle to zero while others intern the same string. Under Miri or
    // ASan a resurrection bug surfaces here; without them it still catches
    // double-free and lost-entry bugs.
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
      .map(|t| {
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
          barrier.wait();
          for i in 0..2000 {
            let key = format!("interner::race/{}", i % 4);
            let a = get(&key);
            assert_eq!(a.as_str(), key);
            let b = a.clone();
            assert!(a.ptr_eq(&b));
            drop(a);
            assert_eq!(b.as_str(), key);
          }
          t
        })
      })
      .collect();
    for thread in threads {
      thread.join().expect("interner race thread panicked");
    }
  }
}
