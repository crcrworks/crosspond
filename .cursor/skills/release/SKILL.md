---
name: release
description: Crosspond 本番リリース。prepare release PR のマージ状態を確認し、必要なら squash merge してから Publish Release ワークフローを起動する。ユーザが /release と入力したときに使う。
disable-model-invocation: true
---

# Publish Release (`/release`)

Crosspond の GitHub Release を実行する。**Publish Release** ワークフローを起動し、タグ作成・.dmg / updater 成果物のアップロードまで CI に任せる。

想定フロー: `/pre-release-review` → `/prepare-release` →（changelog コミット・PR マージ）→ **`/release`**

## 前提

- リポジトリルートで作業する
- `gh` CLI が認証済みであること
- `PUBLIC_CHANGELOG.md` が埋まった prepare PR が存在する（または main にマージ済みである）こと
- GitHub Secrets（Updater 署名鍵、可能なら Apple 署名・公証）が設定済みであること。手順は [README.md](../../../README.md) の Release 節

## 手順

### 1. 最新の prepare release PR を探す

タイトル `chore(main): release *` の PR を検索する:

```bash
gh pr list --search "chore(main): release" --state open --json number,title,state,mergedAt,baseRefName,headRefName
```

オープンがなければマージ済みを含めて直近を確認:

```bash
gh pr list --search "chore(main): release" --state merged --limit 5 --json number,title,state,mergedAt,baseRefName
```

**main 向け**の PR のうち、バージョン番号が最も新しいものを選ぶ。

### 2. PR の状態に応じて分岐

#### A. まだマージされていない（state: OPEN）

1. ユーザに squash merge する旨を短く伝える
2. squash merge を実行:

```bash
gh pr merge <pr-number> --squash
```

3. main が更新されるまで少し待つ（必要なら `gh pr view <pr-number> --json state,mergedAt` で確認）

#### B. すでに main にマージ済み

そのまま次のステップへ。追加の merge は不要。

### 3. main のリリースバージョンを確認（任意・報告用）

```bash
gh api repos/{owner}/{repo}/contents/.release-please-manifest.json?ref=main --jq '.content' | base64 -d
```

`"."` と `crates/crosspond-app/tauri.conf.json` の version が一致していること。

### 4. Publish Release を起動

```bash
gh workflow run "Publish Release"
```

起動直後に実行 URL を取得してユーザに渡す:

```bash
gh run list --workflow="Publish Release" --limit 1 --json databaseId,url,status,displayTitle
```

監視する場合:

```bash
gh run watch <databaseId>
```

失敗時:

```bash
gh run view <databaseId> --log-failed
```

### 5. 完了報告

ユーザに以下を伝える:
- マージした PR 番号（マージした場合）
- リリース対象バージョン
- ワークフロー実行 URL
- 成功すると `v<version>` タグ、GitHub Release、Apple Silicon の .dmg / `.app.tar.gz` / `latest.json` が続くこと

## ワークフロー概要（参考）

**Publish Release** (`publish-release.yml`) は次を行う:
1. main のバージョン整合性と未公開タグを検証
2. fmt / clippy / テスト
3. Release Please で GitHub Release と `v*` タグを作成
4. `PUBLIC_CHANGELOG.md` を Release 本文にする
5. macOS で署名・公証して成果物をその Release に載せる

## やってはいけないこと

- prepare PR が存在しない・changelog が空のまま、ユーザの確認なしに publish を起動する
- merge 以外の方法（rebase merge など）で PR を閉じる — **squash merge のみ**
- ローカルで手動タグ付けや `tauri build` 成果物のアップロードを行う（CI に任せる）

## トラブル時

- **「バージョンが古い」系のエラー**: 新しい prepare PR を作り直す（`/prepare-release`）
- **タグが既に存在**: そのバージョンは publish 済み。次バージョンで prepare からやり直す
- **オープンな prepare PR が複数**: 最新バージョンの PR だけを対象にし、ユーザに確認する
- **TAURI_SIGNING_PRIVATE_KEY missing**: README の Release 節に従い Secrets を入れる
- **公証失敗**: Apple Developer ID の .p12 と App Store Connect API キー（または Apple ID）を確認する
