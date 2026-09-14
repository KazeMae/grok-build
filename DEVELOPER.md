# 开发者指南（中文 / 隐私 overlay）

本仓库是 KazeMae 维护的 grok-build fork，叠了 GrokZen 的简体中文界面与编译期隐私开关。
**没有**移植 GrokZen 的社区自动更新通道和各平台安装器；可执行文件仍是 `grok` / `xai-grok-pager`，`grok update` 仍走官方 x.ai 通道。

普通用户请阅读根目录 [`README.md`](README.md) 与 [`FORK.md`](FORK.md)。

## 社区 overlay 约定

- 程序名：`grok`（构建产物 `xai-grok-pager`）；数据目录仍是 `~/.grok` / `GROK_HOME`。
- 默认 UI locale：`zh-CN`，可用 `--locale en-US` 或 `GROK_ZH_LOCALE` 覆盖。
- 语言包：`crates/codegen/xai-grok-locale`。产品名与路径：`crates/codegen/xai-grok-product`。
- 隐私：`xai-grok-version` 的 `privacy` feature 默认开启。Mixpanel / 产品事件 / OTLP 导出在编译期关掉，不能靠环境变量或远端配置重新打开。
- 自动更新：不要给 `xai-grok-update` 加 `community-build`，不要改官方更新源。

## 从源码构建

```sh
cargo run -p xai-grok-pager-bin
cargo build --locked -p xai-grok-pager-bin --release
./target/release/xai-grok-pager --version
cargo test --locked -p xai-grok-locale -p xai-grok-product
```

Windows debug 构建通过 `xai-grok-pager-bin/build.rs` 把主线程栈留到 8 MB。
