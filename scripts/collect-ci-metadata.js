const { appendFileSync } = require("node:fs")
const { execFileSync } = require("node:child_process")

const { packages } = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], { encoding: "utf8" }))
appendFileSync(process.env.GITHUB_OUTPUT, `has_binaries=${packages.some((pkg) => pkg.targets.some((target) => target.kind.includes("bin")))}\nhas_library=${packages.some((pkg) => pkg.targets.some((target) => target.kind.includes("lib")))}\n`)
