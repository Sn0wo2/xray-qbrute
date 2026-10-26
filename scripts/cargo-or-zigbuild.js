#!/usr/bin/env node
const { createHash } = require("node:crypto")
const { spawnSync } = require("node:child_process")
const { resolve } = require("node:path")

const rustc = spawnSync("rustc", ["-vV"], { encoding: "utf8" })
if (rustc.error || rustc.status !== 0) {
  console.error(`ERROR: rustc -vV failed: ${rustc.error?.message ?? rustc.stderr}`)
  process.exit(1)
}
const host = rustc.stdout?.match(/host:\s+(\S+)/)?.[1]
if (!host) {
  console.error("ERROR: Could not determine host target from rustc -vV")
  process.exit(1)
}

const args = process.argv.slice(2)
const targetIndex = args.findIndex((arg) => arg === "--target" || arg.startsWith("--target="))
const targetArg = args[targetIndex]
const target = targetArg === "--target" ? args[targetIndex + 1] : targetArg?.slice(9)
let env = process.env
if (args[0] === "build" || args[0] === "zigbuild") args.shift()
let command = "build"
if (target === "x86_64-pc-windows-gnu" && target !== host) {
  env = {
    ...env,
    CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER: process.env.CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER ?? "x86_64-w64-mingw32-gcc",
  }
} else if (target && target !== host) {
  env = {
    ...env,
    CARGO_ZIGBUILD_CACHE_DIR: resolve("target", "zigbuild", target, createHash("sha256").update(process.env.RUSTFLAGS ?? "").digest("hex")),
  }
  command = "zigbuild"
}
args.unshift(command)

process.exit(
  spawnSync("cargo", args, {
    stdio: "inherit",
    env,
  }).status ?? 1,
)
