#!/bin/sh
set -eu

# Run from the RuDS repository root. Optional arguments override the generated
# bDS2 site and bDS2 checkout paths.

source_dir=${1:-"$HOME/Blogs/rfc1437.de/html"}
bds2_dir=${2:-"$(dirname "$PWD")/bDS2"}
target_dir="fixtures/golden-generated-sites/rfc1437-sample"

for relative_path in \
  2005/11/13/esmeralda/index.html \
  2026/03/13/cmux-das-terminal-fur-multitasking/index.html \
  2026/03/13/ghostty/index.html \
  atom.xml \
  calendar.json \
  category/article/index.html \
  en/2005/11/13/esmeralda/index.html \
  en/2026/03/13/cmux-das-terminal-fur-multitasking/index.html \
  en/2026/03/13/ghostty/index.html \
  en/index.html \
  index.html \
  rss.xml \
  sitemap.xml
do
  cp "$source_dir/$relative_path" "$target_dir/$relative_path"
  perl -pi -e 's/[ \t]+$//' "$target_dir/$relative_path"
done

cp "$bds2_dir/priv/preview_assets/assets/bds.css" "$target_dir/assets/bds.css"
cp "$bds2_dir/priv/preview_assets/assets/search-runtime.js" "$target_dir/assets/search-runtime.js"
