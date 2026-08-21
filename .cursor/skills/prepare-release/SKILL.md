---
name: prepare-release
description: Crosspond リリース準備。Prepare Release PR ワークフローを起動し、作成された PR ブランチの PUBLIC_CHANGELOG.md 雛形を埋める。ユーザが /prepare-release と入力したときに使う。
disable-model-invocation: true
---

# Prepare Release (`/prepare-release`)

Crosspond の GitHub Release 準備を自動化する。GitHub Actions の **Prepare Release PR** を起動し、作成された PR ブランチで `PUBLIC_CHANGELOG.md` を埋める。**コミット・プッシュは行わない。**

## 前提

- リポジトリルートで作業する
- `gh` CLI が認証済みであること
- VCS はこのリポジトリの慣例に従う（jj なら `jj`、git なら `git`）
- **`/pre-release-review` が通過済み**（Bugbot・Security Review・ローカル CI）。未実施なら先に [pre-release-review](../pre-release-review/SKILL.md) を実行する
- 起動する GitHub ユーザーが repository **admin** であること。ワークフローは `--ref main` と Environment **`release`** を使う

## 手順

### 1. 現在バージョンを確認

`.release-please-manifest.json` の `"."` フィールド（なければ `crates/crosspond-app/tauri.conf.json` の `version`）を読む。

### 2. 次バージョンを決める

デフォルトは **patch** アップ（例: `0.1.0` → `0.1.1`）。

ユーザに確認する:
- 大きな新機能リリースなら **minor** アップを提案
- ユーザが明示したバージョンがあればそれを使う

semver 形式（`X.Y.Z`、v プレフィックスなし）のみ有効。

### 3. Prepare Release PR を起動

```bash
gh workflow run "Prepare Release PR" -f version=<next-version> --ref main
```

起動後、実行を待つ:

```bash
gh run watch "$(gh run list --workflow="Prepare Release PR" --limit 1 --json databaseId --jq '.[0].databaseId')"
```

失敗したらログを確認して停止する:

```bash
gh run view --log-failed
```

### 4. リリース PR を特定

タイトル `chore(main): release` のオープン PR を探す:

```bash
gh pr list --search "chore(main): release" --state open --json number,title,headRefName
```

該当がなければ `--state all` で直近を確認する。複数あれば今回の `<next-version>` に一致するものを選ぶ。

### 5. PR ブランチをチェックアウト

```bash
gh pr checkout <pr-number>
```

ローカルの作業ツリーが PR ブランチになることを確認する。

### 6. PUBLIC_CHANGELOG の雛形を確認

`PUBLIC_CHANGELOG.md` を読み、先頭付近に `## v<next-version> (YYYY-MM-DD)` とプレースホルダーコメント付きセクションがあることを確認する。なければワークフロー完了を待つか、PR の最新コミットを取得する。

### 7. 変更点を把握して changelog を埋める

直前のリリース済みタグ（`v*`）から今回バージョンまでの**最終的な差分**を把握する:

```bash
git tag -l 'v*' --sort=-v:refname | head -1
git log <previous-tag>..HEAD --oneline
```

PR ブランチ上の `CHANGELOG.md`（Release Please 生成）も参考にするが、PUBLIC_CHANGELOG は別物 — ユーザ向けに書き直す。

各セクション（Features / Improvements / Changes / Fixes / Developments）に箇条書きを入れる。**書き方は下記ルールに従う。**

該当がないセクションはプレースホルダーコメントごと削除する（空セクションを残さない）。

### 8. ユーザに提示して終了

- 提案したバージョンと PR 番号・URL
- 埋めた `PUBLIC_CHANGELOG.md` の内容（または差分の要約）
- **コミット・プッシュ・PR 更新はしない** — ユーザが確認・手直ししてから自分でコミットする

## PUBLIC_CHANGELOG の書き方

### ユーザ目線で書く

- 利用者が体験として理解できる言葉で書く
- 「〜できるようになりました」「〜を修正しました」「〜を調整しました」のトーンに揃える
- 技術用語・実装詳細・内部構造は書かない（クレート名、Tauri、sidecar、Release Please、リファクタなど）

### 直前バージョンからの差分だけを書く

- 記載対象は「前回リリース済み → 今回リリース」の間に**最終的に残った変更**のみ
- 開発途中で試したが取り消した変更、脱線して戻った経緯は書かない
- コミット履歴をそのまま列挙せず、ユーザに届く結果としてまとめる

### 新機能はひとまとめに書く（サブ操作を別項目にしない）

今回初めて入った機能について、**使い方・UI・表示の細部を Improvements や Changes に分けて書かない**。

- **書く:** 機能全体を 1 行（必要なら 2 行まで）で要約する
- **書かない:** 新機能の導入に伴う操作手順・配置・見た目を、別セクションの独立した変更として列挙する

Improvements に書いてよいのは、**前回リリース時点ですでに存在していた機能**の使い勝手・見た目・速度の向上だけ。

### Fixes は「既存機能で実際に困った不具合」だけ

Fixes に書くのは、**前回リリース済みのバージョンでユーザがすでに使っていた機能**に残っていた不具合の修正だけ。今回新規追加した機能のリリース前調整は Fixes に書かない。

`CHANGELOG.md` の Bug Fixes をそのまま写さない。

### セクションの使い分け

| セクション | 内容 |
|-----------|------|
| Features | 新しく使えるようになった機能 |
| Improvements | 既存機能の使い勝手・見た目・速度の改善 |
| Changes | 仕様変更・挙動変更（改善でも不具合修正でもないもの） |
| Fixes | **既存機能**で、前バージョンから引き続き存在していた不具合の修正 |
| Developments | ユーザ向けに特筆する告知が必要なときのみ（通常は空でよい） |

## やってはいけないこと

- `jj commit` / `git commit` / `git push` / PR への push
- ユーザの確認なしにバージョンを勝手に minor/major に上げる
- 技術的な changelog をそのまま PUBLIC_CHANGELOG にコピーする
- 新機能の使い方・UI 詳細を、既存機能の Improvements / Changes として別行に書く
- 新機能のリリース前バグ修正を Fixes に書く
