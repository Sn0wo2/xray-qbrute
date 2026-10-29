const { appendFileSync } = require("node:fs")
const { execFileSync } = require("node:child_process")

const { packages } = JSON.parse(execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], { encoding: "utf8" }))
if (
  process.env.VERIFY_VERSION === "true" &&
  packages.some((pkg) =>
    pkg.version !== process.env.VERSION &&
    (pkg.targets.some((target) => target.kind.includes("bin")) || pkg.publish == null || pkg.publish.includes("crates-io")),
  )
) throw new Error(`Expected released packages to have version ${process.env.VERSION}`)
const sha = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim()
if (process.env.GITHUB_EVENT_NAME === "push" && sha !== process.env.GITHUB_SHA) throw new Error("Checkout does not match the pushed tag")
appendFileSync(process.env.GITHUB_OUTPUT, `has_binaries=${packages.some((pkg) => pkg.targets.some((target) => target.kind.includes("bin")))}\nhas_publishable_crates=${packages.some((pkg) => pkg.publish == null || pkg.publish.includes("crates-io"))}\nsha=${sha}\n`)
