#!/bin/bash

# find a cargo.toml file in the current directory, if it does not exist error out
echo "=========================="
echo "corrosionWM nightly script"
echo "=========================="
echo

if [ ! -f Cargo.toml ]; then
    echo "No Cargo.toml file found in current directory"
    exit 1
fi

MAKEFLAGS="-j$(nproc)"
echo "Setting up devenv..."
source .dev.env

# run cargo build --release
cargo build --release

# package the binary into a .zip
# corrosionwm-$(date +%Y-%m-%d).zip
zip corrosionwm-$(date +%Y-%m-%d).zip target/release/corrosionwm

# make a message containing the following:
# ```diff
# get the last commit message
# ```
MESSAGE="
\`\`\`diff
Commit: $(git log -1 --pretty=%B)
Branch: $(git branch --show-current)
====================

$(git diff --stat HEAD^ HEAD)
\`\`\`

Nightly build executed on \`$(date +%Y-%m-%d)\` by \`$(whoami)\`"

USERNAME="corrosionwm nightly build"
PROFILE_PICTURE="https://raw.githubusercontent.com/corrosionwm/corrosionwm/main/corrosionwm.png"

# upload the zip to $DISCORD_WEBHOOK_URL
# use $USERNAME as the username and $PROFILE_PICTURE as the profile picture
# use $MESSAGE as the message
curl -H "Content-Type: multipart/form-data" -F "file=@corrosionwm-$(date +%Y-%m-%d).zip" -F "username=$USERNAME" -F "avatar_url=$PROFILE_PICTURE" -F "content=$MESSAGE" $DISCORD_WEBHOOK_URL

# clean up the zip
rm corrosionwm-$(date +%Y-%m-%d).zip
