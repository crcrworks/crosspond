---
name: pre-release-review
description: リリース前のバグ・セキュリティレビュー。Bugbot と Security Review サブエージェントとローカル CI で main の出荷差分を検証する。ユーザが /pre-release-review と入力したとき、または /prepare-release の直前に使う。
disable-model-invocation: true
---

# Pre-Release Review (`/pre-release-review`)

本番リリース前に、**次バージョンとして出荷する差分**にバグやセキュリティ上の漏れがないか確認する。通過後は `/prepare-release` →（PR マージ）→ `/release` の順で進む。

## ワークフロー上の位置

```
/pre-release-review  →  /prepare-release  →  （changelog コミット・PR マージ）  →  /release
     ↑ このスキル
```

**`/prepare-release` を実行する前に、このレビューを完了すること。**

## 前提

- リポジトリルートで作業する
- VCS はこのリポジトリの慣例に従う（jj なら `jj`、git なら `git`）
- `ui/` に依存関係がインストール済み（`npm --prefix ui install`）

## 手順

### 1. 出荷対象を確定する

1. **`main` を最新にする**（fetch / pull または `jj git fetch` など）
2. **`main` をチェックアウト**する（未マージの feature ブランチ上でレビューしない）
3. **直前のリリースタグ**を取得する:

```bash
git tag -l 'v*' --sort=-v:refname | head -1
```

4. 出荷差分の概要を把握する（参考用。サブエージェントに diff を手計算して渡さない）:

```bash
git log <previous-tag>..HEAD --oneline
```

`<previous-tag>` は手順 3 のタグ（例: `v0.1.0`）。タグが無い初回リリースなら `Base Branch` 行は省略し、デフォルトの base branch 比較に任せる。

### 2. ローカル CI を実行する

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix ui run check
npm --prefix ui test
node --test .github/scripts/public-changelog.test.mjs
```

**失敗したらここで停止**し、修正してから CI を再実行する。`/prepare-release` には進まない。

Linux では workspace clippy の代わりに portable crates だけを見る（`AGENTS.md` の Cursor Cloud 節）。

### 3. Bugbot でバグレビュー

[review-bugbot](~/.cursor/skills-cursor/review-bugbot/SKILL.md) と同じ起動方法で、**ちょうど 1 つ**の `bugbot` サブエージェントを起動する:

- `readonly: true`
- `run_in_background: false`（明示指定がなければ）
- `description: "Bugbot"`
- `subagent_type: "bugbot"`

プロンプト（タグがある場合）:

```text
Full Repository Path: <absolute repository path>
Diff: branch changes
Base Branch: <previous-tag>
Custom Instructions: Pre-release review on main. Focus on regressions, data corruption, secret leakage to the WebView, broken launcher/update flows, and issues that would ship to production users.
```

タグが無い初回リリースの場合は `Base Branch` 行を省略する。

サブエージェントの起動・リトライ・結果の表形式サマリは review-bugbot スキルに従う。

### 4. Security Review でセキュリティレビュー

手順 3 と**並列**で、[review-security](~/.cursor/skills-cursor/review-security/SKILL.md) と同じ起動方法で **ちょうど 1 つ**の `security-review` サブエージェントを起動する:

- `readonly: true`
- `run_in_background: false`
- `description: "Security Review"`
- `subagent_type: "security-review"`

プロンプト（タグがある場合）:

```text
Full Repository Path: <absolute repository path>
Diff: branch changes
Base Branch: <previous-tag>
Custom Instructions: Pre-release review on main. Focus on secret leakage (API keys, ChatGPT tokens, Keychain, updater private keys), WebView exposure, unsigned updater endpoints, and privilege escalation.
```

タグが無い場合は `Base Branch` 行を省略する。

結果のサマリは review-security スキルに従う。

### 5. 手動スモーク観点（該当する変更があるとき）

今回の差分に触れる領域だけ確認する（全部やる必要はない）:

- ランチャーの表示・非表示、IME、Allow / login カード
- Settings のモデル接続（WebView に秘密が出ないこと）
- ブラウザ拡張の Load unpacked パス（バンドルでは app 内の `chrome-extension`）
- 更新チップ（Update available）は `tauri dev` では出なくてよい

### 6. ゲート判定

| 結果 | 扱い |
|------|------|
| CI 失敗 | **ブロック** — 修正して手順 2 から |
| Bugbot / Security の **Critical / High** | **ブロック** — 修正するか、ユーザが明示的にリスク承認するまで `/prepare-release` に進まない |
| **Medium / Low** | 報告し、ユーザが承認すれば続行可 |
| 問題なし | **通過** — 次のステップへ |

レビュー中に見つかった問題を**勝手に修正しない**（ユーザが依頼した場合のみ）。

### 7. 完了報告と次のステップ

通過時、ユーザに簡潔に伝える:

- レビュー範囲（`<previous-tag>..main` または初回リリース）
- CI 結果
- Bugbot / Security の findings 数（表があれば severity 順）
- 手動確認したスモーク項目（あれば）

続行の案内:

> レビュー通過。次は `/prepare-release` でリリース PR と changelog 雛形を用意してください。changelog をコミットして PR をマージしたあと `/release` で GitHub Release を出します。

ブロック時は、修正が必要な finding を severity 順に列挙し、再レビューは `/pre-release-review` の再実行で行う。

## やってはいけないこと

- CI 失敗や未解決の Critical/High finding のまま `/prepare-release` に進む
- feature ブランチだけをレビューして「リリース OK」と判断する（出荷は `main` 基準）
- Bugbot / Security の finding をユーザの確認なしに黙って修正する
