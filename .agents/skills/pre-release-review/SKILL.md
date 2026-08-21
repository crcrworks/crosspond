---
name: pre-release-review
description: リリース前のバグ・セキュリティレビュー。main の出荷差分をローカル CI と読み取り専用のレビューで検証する。/prepare-release の直前、またはユーザが /pre-release-review と指定したときに使う。
---

# Pre-Release Review

`/prepare-release` の前に、次バージョンとして出荷する main の差分を確認する。問題を見つけても、ユーザーが依頼しない限り修正しない。

## 手順

1. 最新の main を取得し、main 上で作業する。未コミット変更がある場合は保護する。
2. `git tag -l 'v*' --sort=-v:refname | head -1` で直前タグを確認し、`git log <previous-tag>..HEAD --oneline` で範囲を把握する。
3. 次を実行する。失敗したらブロックし、修正後に再レビューする。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix ui run check
npm --prefix ui test
node --test .github/scripts/public-changelog.test.mjs
```

4. 可能なら読み取り専用の bug review と security review を各 1 回、並列で依頼する。専用レビュー担当が使えない場合は、差分を読み取り専用で自分で確認する。
   - Bug: 回帰、データ破壊、ランチャー/更新フロー、WebView への秘密漏洩。
   - Security: API キー / ChatGPT トークン / Keychain、updater 秘密鍵、未署名の更新エンドポイント。
5. CI 失敗、または Critical/High の finding はブロックする。Medium/Low は報告し、承認がある場合だけ続行可。

## 完了報告

レビュー範囲、CI 結果、レビュー finding を severity 順、実施したスモーク項目を簡潔に報告する。通過時は「次は `/prepare-release`」と案内する。ブロック時は再レビューが必要な finding を列挙する。
