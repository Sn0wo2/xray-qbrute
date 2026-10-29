const { chmodSync, copyFileSync, mkdirSync, readFileSync } = require("node:fs")
const { join } = require("node:path")

const artifacts = JSON.parse(readFileSync("dist/artifacts.json", "utf8"))
for (const [buildId, target, destination] of [
  ["linux-windows-amd64-v2", "x86_64-unknown-linux-gnu", "linux/amd64"],
  ["linux-arm", "aarch64-unknown-linux-gnu", "linux/arm64"],
]) {
  const matches = artifacts.filter((artifact) =>
    artifact.type === "Binary" &&
    artifact.extra?.ID === buildId &&
    artifact.extra?.Binary === "xray-qbrute" &&
    artifact.target === target,
  )
  if (matches.length !== 1) throw new Error("expected one matching binary artifact")

  mkdirSync(join("container-context", destination), { recursive: true })
  copyFileSync(matches[0].path, join("container-context", destination, "xray-qbrute"))
  chmodSync(join("container-context", destination, "xray-qbrute"), 0o755)
}
