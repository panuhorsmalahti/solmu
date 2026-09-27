#!/bin/sh
set -eu
SOLMU_COMPONENT=muxer
export SOLMU_COMPONENT
installer=$(curl -fsSL "${SOLMU_INSTALLER_BASE_URL:-https://raw.githubusercontent.com/panuhorsmalahti/solmu/main/scripts}/install.sh")
printf '%s\n' "$installer" | sh
