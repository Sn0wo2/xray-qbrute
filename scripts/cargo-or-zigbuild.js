#!/usr/bin/env node
const { createHash } = require("node:crypto")
const { spawnSync } = require("node:child_process")
const { copyFileSync, mkdirSync, rmSync } = require("node:fs")
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
const target = args[targetIndex] === "--target" ? args[targetIndex + 1] : args[targetIndex]?.slice(9)
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

const binIndex = args.findIndex((arg) => arg === "--bin" || arg.startsWith("--bin="))
const bin = args[binIndex] === "--bin" ? args[binIndex + 1] : args[binIndex]?.slice(6)
const targetPrefix = target?.split(".", 1)[0]
const abi = targetPrefix?.slice(targetPrefix.lastIndexOf("-") + 1)
const outputTarget = target && abi?.startsWith("gnu") && abi !== "gnullvm" ? targetPrefix : target
if (process.env.BUILD_PROFILE === "debug" && (!bin || !outputTarget)) {
  console.error("ERROR: debug builds require a binary name and target")
  process.exit(1)
}
if (process.env.BUILD_PROFILE === "debug") {
  const release = args.indexOf("--release")
  if (release !== -1) args.splice(release, 1)
  args.push("--profile=dev")
}
if (process.env.BUILD_PROFILE === "debug") {
  rmSync(resolve("target", outputTarget, "release", `${bin}${target?.includes("windows") ? ".exe" : ""}`), { force: true })
}
const result = spawnSync("cargo", args, { stdio: "inherit", env })
if (process.env.BUILD_PROFILE === "debug" && result.status === 0) {
  mkdirSync(resolve("target", outputTarget, "release"), { recursive: true })
  copyFileSync(
    resolve("target", outputTarget, "debug", `${bin}${target?.includes("windows") ? ".exe" : ""}`),
    resolve("target", outputTarget, "release", `${bin}${target?.includes("windows") ? ".exe" : ""}`),
  )
}
process.exit(result.status ?? 1)
