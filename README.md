# xray-qbrute

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/Sn0wo2/xray-qbrute)

> Brute-force Xray-core Telegram group question 26/7/14

## About

  xray-qbrute 是帮助你计算 [Xray-core](https://github.com/XTLS/Xray-core) [Telegram](https://t.me/projectXray) 入群问题的工具

  开发思路可参见 [讨论过程](https://github.com/XTLS/Xray-core/discussions/6451)

  若 [Xray-core](https://github.com/XTLS/Xray-core) [Telegram](https://t.me/projectXray) 更换题目, 不保证继续维护; 截至 26/7/14 可用

## Download

  从 [Releases](https://github.com/Sn0wo2/xray-qbrute/releases) 获取各平台的预编译二进制与 checksums.txt; 容器镜像发布于 [ghcr.io/sn0wo2/xray-qbrute](https://github.com/Sn0wo2/xray-qbrute/pkgs/container/xray-qbrute)

  网页版 (Wasm) 会由 CI 自动部署至 GitHub Pages; 尚无 Release 时, 可前往 [CI Actions](https://github.com/Sn0wo2/xray-qbrute/actions/workflows/ci.yml), 在「Artifacts」中下载「xray-qbrute-linux-windows-builds」或「xray-qbrute-macos-builds」快照构建

## Thanks

- [@megapro17](https://github.com/megapro17) 的 [xray-pow](https://github.com/megapro17/xray-pow) 与 [讨论过程](https://github.com/XTLS/Xray-core/discussions/6451#discussioncomment-17755032) 中提供的思路
- [@bytecategory](https://github.com/bytecategory) 的 [𝔠𝔯𝔭𝔱𝔬𝔭𝔢𝔯𝔰𝔢𝔰](http://bytecategory.com:3000/) 与其提供的思路
- [@CptTZ](https://github.com/CptTZ) 提供的 [CUDA](https://github.com/CptTZ/random/tree/main/2026/xray-group-question) 思路

## Contribution

- [@XiaoQing235](https://github.com/XiaoQing235) 贡献的 wgpu & avx2 硬件加速 [#1](https://github.com/Sn0wo2/xray-qbrute/pull/1) [#3](https://github.com/Sn0wo2/xray-qbrute/pull/3)
- [@Terminal-Void](https://github.com/Terminal-Void) 贡献的 MacOS Metal 管线硬件加速支持 [#2](https://github.com/Sn0wo2/xray-qbrute/pull/2)
