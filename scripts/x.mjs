#!/usr/bin/env zx

import "zx/globals";

import { Command } from "commander";

import { publish_handler } from "./publish.mjs";
import { version_handler } from "./version.mjs";
import {prepublish_handler} from "./prepublish.mjs";

process.env.CARGO_TERM_COLOR = "always"; // Assume every terminal that using zx supports color
process.env.FORCE_COLOR = 3; // Fix zx losing color output in subprocesses

const program = new Command();

program
    .name("Rspack Resolve Utils CLI")
    .description("CLI for development of Rspack Resolve")
    .showHelpAfterError(true)
    .showSuggestionAfterError(true);

program
    .command("publish")
    .requiredOption("--tag <char>", "publish tag")
    .option(
        "--dry-run",
        "Does everything a publish would do except actually publishing to the registry"
    )
    .option("--no-dry-run", "negative dry-run")
    .option("--push-tags", "push tags to github")
    .option("--no-push-tags", "don't push tags to github")
    .option("--otp", "use npm OTP auth")
    .description("publish package after version bump")
    .action(publish_handler);

program
    .command("version")
    .argument("<bump_version>", "bump version to (major|minor|patch)")
    .option("--pre <string>", "pre-release tag")
    .description("bump version")
    .action(version_handler);

program
    .command("prepublish")
    .description("prepublishOnly")
    .action(prepublish_handler);

let argv = process.argv.slice(2); // remove the `node` and script call
if (argv[0] && /x.mjs/.test(argv[0])) {
    // Called from `zx x.mjs`
    argv = argv.slice(1);
}
program.parse(argv, { from: "user" });



