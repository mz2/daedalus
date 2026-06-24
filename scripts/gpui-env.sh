#!/usr/bin/env bash
# Expose the system libraries GPUI's platform stack needs (fontconfig, freetype, xkbcommon,
# wayland, xcb/X11, vulkan) from the Nix store, for building/running with `--features gpui`.
#
# Usage:  source scripts/gpui-env.sh
#         cargo run -p daedalus-desktop --features gpui
#
# On a non-Nix distro, install the -dev packages instead (fontconfig, freetype, libxkbcommon,
# wayland, libxcb/X11, vulkan-loader) and this script is unnecessary.

# Curated package families (keeps PKG_CONFIG_PATH small enough to stay under ARG_MAX).
_families="fontconfig freetype libxkbcommon wayland libxcb libx11 libxfixes libxrandr \
libxi libxcursor libxext libxrender xorgproto vulkan-loader libglvnd mesa"

_add_unique() { # $1=current list  $2=candidate -> echoes new list
    case ":$1:" in *":$2:"*) echo "$1" ;; *) echo "${1:+$1:}$2" ;; esac
}

_pcp=""
for fam in $_families; do
    for d in /nix/store/*"$fam"*-dev/lib/pkgconfig /nix/store/*"$fam"*/lib/pkgconfig \
             /nix/store/*"$fam"*/share/pkgconfig; do
        [ -d "$d" ] && _pcp="$(_add_unique "$_pcp" "$d")"
    done
done

_ldp=""
for fam in $_families; do
    for d in /nix/store/*"$fam"*/lib; do
        [ -d "$d" ] && _ldp="$(_add_unique "$_ldp" "$d")"
    done
done

export PKG_CONFIG_PATH="${_pcp}${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
# LIBRARY_PATH = link-time `-l` search (the .so live in the non-dev outputs on Nix);
# LD_LIBRARY_PATH = the same dirs for run time.
export LIBRARY_PATH="${_ldp}${LIBRARY_PATH:+:$LIBRARY_PATH}"
export LD_LIBRARY_PATH="${_ldp}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

# Point the Vulkan loader at mesa's ICD manifest(s) if present.
_icds=""
for j in /nix/store/*mesa*/share/vulkan/icd.d/*.json; do
    [ -f "$j" ] && _icds="${_icds:+$_icds:}$j"
done
[ -n "$_icds" ] && export VK_ICD_FILENAMES="$_icds"

echo "gpui-env: PKG_CONFIG_PATH ($(echo "$PKG_CONFIG_PATH" | tr ':' '\n' | grep -c .) dirs), LD_LIBRARY_PATH set${VK_ICD_FILENAMES:+, VK_ICD set}."
unset _families _pcp _ldp _icds
