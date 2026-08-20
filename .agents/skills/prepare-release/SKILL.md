---
name: prepare-release
description: Crosspond リリース準備。Prepare Release PR ワークフローを起動し、作成された PR ブランチの PUBLIC_CHANGELOG.md 雛形を埋める。ユーザが /prepare-release と入力したときに使う。
---

# Prepare Release

コミット・プッシュ・PR 更新は行わず、ユーザが確認できる状態まで準備する。

## 前提

- `/pre-release-review` が通過済みであること。
- `gh` が認証済みであること。
- 作業ツリーにユーザーの未保存変更があれば、ブランチ切り替え前に確認する。

## 手順

1. `.release-please-manifest.json` の `"."`、なければ `crates/crosspond-app/tauri.conf.json` の version を読む。デフォルトは patch アップ。minor/major または明示バージョンがある場合はそれに従う。semver は `X.Y.Z`（v なし）。
2. `gh workflow run "Prepare Release PR" -f version=<next-version> --ref main` を実行し、`gh run watch` で完了を待つ。失敗時は `gh run view --log-failed` を確認して停止する。repository admin 以外、または `release` environment 未設定では失敗する。
3. `gh pr list --search "chore(main): release" --state open --json number,title,headRefName` で対象 PR を特定し、`gh pr checkout <pr-number>` する。
4. `PUBLIC_CHANGELOG.md` の先頭にある `## v<version> (YYYY-MM-DD)` の雛形を確認する。
5. 直前の `v*` タグから最終差分を確認し、Features / Improvements / Changes / Fixes / Developments をユーザー向けの日本語で埋める。該当しないセクションは削除する。
6. 提案バージョン、PR 番号・URL、changelog の要約を報告して終了する。

## Changelog の基準

- 新機能は Features に機能全体を 1〜2 行でまとめ、操作や UI の細部を別項目に分けない。
- Improvements は前回リリース時点ですでに存在した機能の改善だけ。
- Fixes は前回リリース時点ですでに存在した機能の不具合だけ。新機能の初期実装の修正は含めない。
- クレート名、Tauri、sidecar、Release Please などの実装用語は書かない。

公開されていない changelog のまま PR をマージしたり、ユーザー確認なしに minor/major を選んだりしない。
