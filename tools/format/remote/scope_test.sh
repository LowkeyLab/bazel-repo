#!/usr/bin/env bash
set -euo pipefail
snapshot=$(realpath "$1")
mkdir "$TEST_TMPDIR/repo"
cd "$TEST_TMPDIR/repo"
git init -q
printf 'ignored\n' >.gitignore
touch tracked deleted skipped ignored 'new file'
git add .gitignore tracked deleted skipped
git update-index --skip-worktree skipped
rm deleted
if bash "$snapshot" "$TEST_TMPDIR/input.tar"; then
	printf 'Unreviewed regular file accepted\n' >&2
	exit 1
fi
git add -- 'new file'
mkdir -p nested/dir "$TEST_TMPDIR/outside/dir"
touch nested/dir/file "$TEST_TMPDIR/outside/dir/file"
git add nested/dir/file
rm -r nested
ln -s "$TEST_TMPDIR/outside" nested
ln -s missing dangling
ln -s "$TEST_TMPDIR/outside/dir/file" linked
git add dangling
bash "$snapshot" "$TEST_TMPDIR/input.tar"
tar -tf "$TEST_TMPDIR/input.tar" | LC_ALL=C sort >actual
printf '%s\n' .gitignore 'new file' tracked >expected
diff -u expected actual
