# OpenFX OMT Plugin

DaVinci Resolve 向けの映像専用 OpenFX フィルターです。

## 要件

- Windows x64
- DaVinci Resolve（OpenFX ホスト）

## インストール

[Releases](https://github.com/MikanseiLaboratory/openfx-omt-plugin/releases) の `openfx-omt-plugin-v*.zip` を展開し、`OpenFXOMT.ofx.bundle` を次の場所へコピーします。

```text
C:\Program Files\Common Files\OFX\Plugins
```

## 使い方

1. クリップに **OMT Output** を適用する
2. **Enabled** をオンにする
3. **Source Name** と **Quality**（Default / Low / Medium / High）を必要なら変更する
4. 受信側で `HOSTNAME (ソース名)` または `omt://IP:ポート` を選ぶ
