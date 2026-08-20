---
name: release
description: Crosspond 本番リリース。prepare release PR の状態を確認し、必要なら squash merge して Publish Release ワークフローを起動する。ユーザが /release と入力したときに使う。
---

# Publish Release

想定フローは `/pre-release-review` → `/prepare-release` → changelog 確認・PR マージ → `/release`。

## 手順

1. `gh pr list --search "chore(main): release" --state open --json number,title,state,mergedAt,baseRefName,headRefName` で open PR を探す。なければ merged の直近 5 件を調べ、main 向けで最も新しいバージョンを選ぶ。
2. 未マージなら、対象 PR と squash merge を行うことをユーザーに短く伝えてから `gh pr merge <pr-number> --squash` を実行する。複数候補や changelog 未確認なら停止して確認を求める。
3. `gh workflow run "Publish Release" --ref main` を実行し、`gh run list --workflow="Publish Release" --limit 1 --json databaseId,url,status,displayTitle` で URL を取得する。必要なら `gh run watch <databaseId>`、失敗時は `gh run view <databaseId> --log-failed`。Write 権限だけでは失敗する。成功するまで Release は draft のまま。
4. 対象バージョン、マージした PR、ワークフロー URL を報告する。成功後は `v<version>` タグ、公開 GitHub Release、Apple Silicon の署名済み .dmg が CI から続くことも伝える。`UNLICENSED` のままなら初回公開前にライセンス選択が必要なことを伝える。

ローカルでタグ付けや成果物アップロードをしない。prepare PR や changelog が確認できない場合は publish を起動しない。
