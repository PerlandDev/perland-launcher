#!/usr/bin/env bash
# Perland Launcher: installs Ubuntu packages on a CI runner, e.g.
#   scripts/ci/apt-install.sh libdbus-1-dev pkg-config
# A runner's package mirror can stall without an error, and a bare `apt-get update` then waits
# until the job's timeout. Here each call is capped (APT_UPDATE_TIMEOUT, APT_INSTALL_TIMEOUT),
# apt retries and times out its own downloads, and the whole thing is tried three times before
# failing with an error that says what happened.
# A GitHub runner lists its own mirror first (azure.archive.ubuntu.com) and Ubuntu's archive after
# it, and that mirror can be down for hours. After a failed attempt it is taken off the list
# (APT_MIRROR_LIST), so the next attempts go to Ubuntu's archive.
set -u
[ "$#" -gt 0 ] || { echo "usage: $0 <package>..." >&2; exit 2; }
opts=(-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)
mirror_list="${APT_MIRROR_LIST:-/etc/apt/apt-mirrors.txt}"
runner_mirror='azure\.archive\.ubuntu\.com'
skip_runner_mirror() {
  # Only when another mirror is left to use.
  [ -f "$mirror_list" ] && grep -q "$runner_mirror" "$mirror_list" &&
    grep -v "$runner_mirror" "$mirror_list" | grep -q '[^[:space:]]' || return 0
  echo "::warning::Leaving out the runner's package mirror for the next attempt."
  sudo sed -i "/$runner_mirror/d" "$mirror_list"
}
for attempt in 1 2 3; do
  if sudo timeout "${APT_UPDATE_TIMEOUT:-3m}" apt-get "${opts[@]}" update &&
     sudo timeout "${APT_INSTALL_TIMEOUT:-6m}" apt-get "${opts[@]}" install -y "$@"; then
    exit 0
  fi
  echo "::warning::Installing $* failed or stalled (attempt $attempt of 3)."
  [ "$attempt" = 3 ] || { skip_runner_mirror; sleep "${APT_RETRY_DELAY:-15}"; }
done
echo "::error::Couldn't install $*: the runner's package mirror isn't answering. Re-run the job."
exit 1
