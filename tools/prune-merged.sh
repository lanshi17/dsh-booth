#!/usr/bin/env bash
# Delete every branch a merged pull request left behind, locally and on the remote.
#
#   tools/prune-merged.sh                       list what would go (the default)
#   tools/prune-merged.sh --apply               delete the merged local and remote branches
#   tools/prune-merged.sh --apply --worktrees   also remove the clean worktrees that hold them
#
# A branch counts as merged only when GitHub reports its pull request MERGED and the branch's
# tip is still the head commit GitHub merged. This workflow merges with squash commits, which
# `git branch --merged` cannot see, so history is never consulted. The current branch, master/main, and any branch a worktree holds are never
# deleted unless the worktree itself is removed first.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2

remote=origin
limit=500
apply=
worktrees=

usage() {
  cat <<'EOF'
usage: tools/prune-merged.sh [options]

  --apply              delete the branches (the default is a dry run)
  --worktrees          with --apply, remove the clean worktrees that hold merged branches
  --remote <name>      remote to prune (default: origin)
  --limit <n>          merged pull requests to read from GitHub (default: 500)
  -h, --help           show this text
EOF
}

die() {
  printf 'prune-merged: %s\n' "$1" >&2
  exit 2
}

while [ $# -gt 0 ]; do
  case $1 in
    --apply) apply=1 ;;
    --worktrees) worktrees=1 ;;
    --remote)
      shift
      [ $# -gt 0 ] || die '--remote needs a name'
      remote=$1
      ;;
    --limit)
      shift
      [ $# -gt 0 ] || die '--limit needs a number'
      limit=$1
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

git rev-parse --git-dir >/dev/null 2>&1 || die 'not inside a git repository'
git remote get-url "$remote" >/dev/null 2>&1 || die "no remote named $remote"
command -v gh >/dev/null 2>&1 || die 'gh is not installed (https://cli.github.com)'
gh auth status >/dev/null 2>&1 || die 'gh is not signed in; run: gh auth login'

git fetch "$remote" --prune --quiet || die "cannot fetch $remote"

owner=$(gh repo view --json owner --jq '.owner.login' 2>/dev/null) || owner=

# "branch<TAB>head sha<TAB>owner" per merged pull request. The name alone is not enough: a
# fork's PR can share a branch name, and a branch that kept moving after its merge is not the
# branch GitHub merged.
prs=$(gh pr list --state merged --limit "$limit" \
  --json headRefName,headRefOid,headRepositoryOwner \
  --jq '.[] | "\(.headRefName)\t\(.headRefOid)\t\(.headRepositoryOwner.login)"') ||
  die "cannot read merged pull requests; is $remote a GitHub repository?"

if [ -z "$prs" ]; then
  echo 'no merged pull requests; nothing to prune'
  exit 0
fi

# merged_head <branch> — the head commit of the newest merged PR for this branch, ignoring
# PRs whose head lives in a fork. Empty when no such PR exists.
merged_head() {
  printf '%s\n' "$prs" | awk -F '\t' -v branch="$1" -v owner="$owner" '
    $1 == branch && (owner == "" || $3 == owner) { print $2; exit }'
}

current=$(git symbolic-ref --quiet --short HEAD || true)

# One "branch<TAB>path" line per worktree that has a branch checked out.
worktree_pairs=$(git worktree list --porcelain | awk '
  /^worktree / { path = substr($0, 10) }
  /^branch /   { sub("^branch refs/heads/", ""); print $0 "\t" path }
')

worktree_of() {
  printf '%s\n' "$worktree_pairs" | awk -F '\t' -v branch="$1" '$1 == branch { print $2; exit }'
}

local_delete=
local_kept=
worktree_delete=
remote_delete=
failed=0

# plan <kind> <branch> <worktree path or empty>
plan() {
  kind=$1
  branch=$2
  wt=${3-}
  if [ -z "$apply" ]; then
    if [ "$kind" = local ]; then
      printf 'would delete local  %s\n' "$branch"
    else
      printf 'would remove worktree %s (%s)\n' "$wt" "$branch"
    fi
    return 0
  fi

  case $kind in
    worktree)
      if git worktree remove --force --force "$wt"; then
        printf 'removed worktree %s (%s)\n' "$wt" "$branch"
      else
        printf 'could not remove worktree %s\n' "$wt" >&2
        failed=1
      fi
      ;;
    local)
      if git branch -D "$branch" >/dev/null; then
        printf 'deleted local  %s\n' "$branch"
      else
        printf 'could not delete local %s\n' "$branch" >&2
        failed=1
      fi
      ;;
  esac
}

while IFS= read -r name; do
  [ -n "$name" ] || continue
  case $name in
    HEAD | master | main) continue ;;
  esac
  [ "$name" = "$current" ] && continue
  head=$(merged_head "$name")
  [ -n "$head" ] || continue
  if [ "$(git rev-parse --verify --quiet "refs/heads/$name")" != "$head" ]; then
    printf 'keeping %s: it is no longer the branch the pull request merged\n' "$name" >&2
    continue
  fi

  wt=$(worktree_of "$name")
  if [ -n "$wt" ]; then
    if [ -z "$worktrees" ]; then
      local_kept="${local_kept}${name}"$'\n'
      continue
    fi
    if [ -n "$(git -C "$wt" status --porcelain 2>/dev/null)" ]; then
      printf 'keeping %s: its worktree %s has changes\n' "$name" "$wt" >&2
      local_kept="${local_kept}${name}"$'\n'
      continue
    fi
    worktree_delete="${worktree_delete}${name}|${wt}"$'\n'
  fi
  local_delete="${local_delete}${name}"$'\n'
done < <(git for-each-ref --format='%(refname:short)' refs/heads)

while IFS= read -r ref; do
  [ -n "$ref" ] || continue
  git symbolic-ref --quiet "$ref" >/dev/null && continue # origin/HEAD and aliases
  name=${ref#"refs/remotes/$remote/"}
  case $name in
    HEAD | master | main) continue ;;
  esac
  head=$(merged_head "$name")
  [ -n "$head" ] || continue
  tip=$(git rev-parse --verify --quiet "refs/remotes/$remote/$name") || continue
  if [ "$tip" != "$head" ]; then
    printf 'keeping %s/%s: it is no longer the branch the pull request merged\n' "$remote" "$name" >&2
    continue
  fi
  remote_delete="${remote_delete}${name}"$'\n'
done < <(git for-each-ref --format='%(refname)' "refs/remotes/$remote")

while IFS='|' read -r branch wt; do
  [ -n "$branch" ] || continue
  plan worktree "$branch" "$wt"
done <<EOF
$worktree_delete
EOF

while IFS= read -r branch; do
  [ -n "$branch" ] || continue
  plan local "$branch"
done <<EOF
$local_delete
EOF

if [ -n "$remote_delete" ]; then
  IFS=$'\n'
  # Branch names cannot contain a newline, so this is one argument per branch.
  set -- $remote_delete
  IFS=$' \t\n'
  if [ -n "$apply" ]; then
    if git push "$remote" --delete "$@" >/dev/null 2>&1; then
      for branch in "$@"; do printf 'deleted remote %s/%s\n' "$remote" "$branch"; done
    else
      printf 'could not delete the remote branches\n' >&2
      failed=1
    fi
  else
    for branch in "$@"; do printf 'would delete remote %s/%s\n' "$remote" "$branch"; done
  fi
fi

if [ -n "$apply" ]; then
  git worktree prune
fi

if [ -n "$local_kept" ]; then
  printf '\nkept (a worktree holds them; rerun with --worktrees, or clean it first):\n' >&2
  printf '%s' "$local_kept" | sed 's/^/  /' >&2
fi

count() { printf '%s' "$1" | grep -c . || true; }
printf '\n%s %d local, %d remote\n' \
  "$([ -n "$apply" ] && echo deleted || echo 'would delete')" \
  "$(count "$local_delete")" "$(count "$remote_delete")"

exit "$failed"
