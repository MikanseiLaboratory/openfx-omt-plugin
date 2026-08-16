# OpenFX OMT Plugin

DaVinci Resolve 向けの映像専用 OpenFX フィルターです。タイムラインの絵を変えずに、有効時だけ Open Media Transport (OMT) で送出します。

## 要件

- Windows x64
- DaVinci Resolve（OpenFX ホスト）
- 映像のみ（OpenFX Image Effect では音声を扱えません）

## インストール

[Releases](https://github.com/MikanseiLaboratory/openfx-omt-plugin/releases) の `openfx-omt-plugin-v*.zip` を展開し、`OpenFXOMT.ofx.bundle` を次の場所へコピーします。

```text
C:\Program Files\Common Files\OFX\Plugins
```

Resolve を起動し直すと、OpenFX の **Mikansei Laboratory / OMT Output** から使えます。

## 使い方

1. クリップに **OMT Output** を適用する
2. **Enabled** をオンにする
3. **Source Name** と **Quality**（Default / Low / Medium / High）を必要なら変更する
4. 受信側で `HOSTNAME (ソース名)` または `omt://IP:ポート` を選ぶ

映像は常に入力から出力へそのままコピーされます。Enabled がオフのときは送出しません。色空間は BT.709 です。

受信側のファイアウォールは UDP mDNS と TCP `6400`–`6600` を許可してください。

## 削除

Resolve を終了してから `C:\Program Files\Common Files\OFX\Plugins\OpenFXOMT.ofx.bundle` を削除します。

## 開発

ビルドには Rust 1.97 と LLVM（bindgen / libclang）が必要です。

```powershell
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked -- --test-threads=1
cargo build --release --locked --target x86_64-pc-windows-msvc
./scripts/package.ps1
```

Resolve での確認手順は `docs/resolve-verification.md` です。

## ライセンス

MIT。第三者通知は `THIRD_PARTY_NOTICES.md`。OpenFX ヘッダーは Academy Software Foundation の BSD-3-Clause です。
