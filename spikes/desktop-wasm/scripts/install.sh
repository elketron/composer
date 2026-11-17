#!/usr/bin/env bash
# Ticket 0206: put the spike where it belongs — the deliverable is spikes/desktop-wasm/ in the repo.
# The scripts stay a second copy under the coding site (a composer-harness worker can only write to
# a site with its own tools); this copies the site copy into the worktree and reports what changed.
set -uo pipefail
wt="${1:?usage: install.sh <worktree> <coding-site scratch dir>}"
src="${2:?}"
dst="$wt/spikes/desktop-wasm"
mkdir -p "$dst/scripts"
cp "$src/FINDINGS.md" "$dst/FINDINGS.md"
for f in make-probes.sh make-probes2.sh make-probes3.sh make-link-test.sh run-probes.sh \
         analyze.sh module-split.sh counts.sh blockers.sh gpui-web.sh serving.sh \
         browser-client.sh release-and-terms.sh signing-and-tools.sh sign-narrow.sh sign-check.sh \
         nightly-gpui.sh install.sh; do
  [ -f "$src/$f" ] && cp "$src/$f" "$dst/scripts/$f"
done
# probe*/target is cargo scratch, never a deliverable
printf 'target/\nweb/\n' > "$dst/.gitignore"
rm -rf "$dst/probe/target" "$dst/probe2/target" "$dst/probe3/target"
{
  echo '# what is in the spike'
  find "$dst" -maxdepth 2 -not -path '*/target*' | sed "s#$wt/##" | sort
  echo
  echo "# size: $(du -sh "$dst" | cut -f1)"
  echo
  echo '# Nothing outside spikes/ was written by this ticket.'
  echo '# git is unreachable from this sandbox (ticket 0203'"'"'s mount needs a rebuilt, restarted
  # server), so instead of `git diff --name-only main..HEAD` this compares the tree against the
  # spike'"'"'s first artefact: everything in the worktree carries the 10:20 checkout timestamp, and
  # the first file this ticket wrote is logs/client-wasm.txt at 10:26.'
  echo '# command: find <worktree> -newer spikes/desktop-wasm/logs/client-wasm.txt -type f \'
  echo '#            -not -path "*/spikes/*" -not -path "*/target/*" -not -path "*/.cef/*"'
  find "$wt" -newer "$dst/logs/client-wasm.txt" -type f \
    -not -path "$dst/*" -not -path '*/target/*' -not -path '*/.cef/*' \
    | sed "s#$wt/##" | head -20
  echo '# (empty above = only spikes/desktop-wasm/ changed; apps/desktop/target and apps/desktop/.cef
  # are gitignored build output, listed in apps/desktop/.gitignore)'
} > "$dst/CONTENTS.txt"
cat "$dst/CONTENTS.txt"
