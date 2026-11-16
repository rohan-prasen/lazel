#!/bin/sh
set -eu

# Run manually in a Docker container with its own network namespace and NET_ADMIN.
if [ ! -f /.dockerenv ] || [ "$(id -u)" -ne 0 ]; then
    printf '%s\n' 'Run this script with sudo inside the Lazel Docker dev container.' >&2
    exit 1
fi

if ! nft -f - <<'RULES'
add table inet lazel_devcontainer
delete table inet lazel_devcontainer
table inet lazel_devcontainer {
    chain input {
        type filter hook input priority filter; policy drop;
        iifname "lo" accept
        ct state established,related accept
        meta l4proto ipv6-icmp accept
    }
}
RULES
then
    printf '%s\n' 'Could not apply the container firewall. Check that the container has NET_ADMIN.' >&2
    exit 1
fi

printf '%s\n' 'Container firewall enabled. Outbound connections remain available.'
