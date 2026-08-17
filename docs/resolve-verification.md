# DaVinci Resolve 手動検証

対象: Windows x64、DaVinci Resolve（Studio / Free）。映像専用 OpenFX フィルターです。音声は扱いません。

## 導入

1. `OpenFXOMT.ofx.bundle` を `C:\Program Files\Common Files\OFX\Plugins` にコピーする（`./scripts/install.ps1` でも可）
2. Resolve を起動し直す
3. Fusion または Color の OpenFX 一覧に **Mikansei Laboratory / OMT Output** が出ることを確認する

## 検出と UI

- フィルターをクリップに適用すると **Enabled**、**Source Name**、**Quality**（Default / Low / Medium / High）が表示される
- 既定値は Enabled = on、Source Name = `DaVinci Resolve`、Quality = Default

## パススルー

1. カラーバーや既知のクリップにフィルターを当てる
2. Enabled をオフにしてもオンにしても、ビューアの絵が入力と一致する（フィルターは絵を変えない）
3. Quality や Source Name を変えても絵は変わらない

## 再生中の受信

1. Enabled をオンにする
2. OMT 受信側（OBS、vMix、`omt-tools` など）で `HOSTNAME (DaVinci Resolve)` または `omt://IP:ポート` を選ぶ
3. タイムラインを再生すると受信側に映像が流れ、停止すると最後のフレーム付近で止まる
4. Enabled をオフにすると受信が止まる。オンに戻すと再び送出される

## 再設定と複数インスタンス

- Source Name や Quality を変えると送出が一度止まって新しい設定で再開する。Resolve が落ちないこと
- 同じプロジェクトにフィルターを 2 つ以上置き、ソース名を別々にする。両方受信でき、片方を消してももう片方が残る
- 再生停止、クリップ削除、プロジェクトを閉じたあと、タスク マネージャーに `openfx-omt-sender` 相当の送信スレッドや Resolve の残留が増え続けないこと

## アンインストール

1. Resolve を終了する
2. `C:\Program Files\Common Files\OFX\Plugins\OpenFXOMT.ofx.bundle` を削除する
3. Resolve を起動し、プラグイン一覧から消えていることを確認する
