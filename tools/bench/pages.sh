#!/bin/bash
set -uo pipefail
out=${BENCH_PAGES:-$HOME/.cache/bande-web/pages}
agent='Mozilla/5.0 (X11; Linux aarch64; rv:153.0) Gecko/20100101 Firefox/153.0'
pages='
wiki-rust https://en.wikipedia.org/wiki/Rust_(programming_language)
wiki-cat https://en.wikipedia.org/wiki/Cat
mdn-grid https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_grid_layout/Basic_concepts_of_grid_layout
rust-vec https://doc.rust-lang.org/std/vec/struct.Vec.html
html-parsing https://html.spec.whatwg.org/multipage/parsing.html
hackernews https://news.ycombinator.com/
guardian https://www.theguardian.com/international
github-rust https://github.com/rust-lang/rust
lwn https://lwn.net/
'
while read -r name url; do
    [ -n "$name" ] || continue
    [ ! -e "$out/$name/.done" ] || continue
    rm -rf "${out:?}/$name"
    mkdir -p "$out/$name"
    wget -q -e robots=off -U "$agent" --timeout=20 --tries=2 -p -k -E -H -nd --no-host-directories \
        --reject-regex '\.(js|mjs|webm|mov|mp4|ogv|ogg)(\?|$)' -P "$out/$name" "$url"
    first=$(find "$out/$name" -maxdepth 1 -type f -name '*.html' -printf '%T@ %s %p\n' | sort -n | head -1 | cut -d' ' -f3-)
    [ -n "$first" ] || { echo "pages: no document for $name" >&2; continue; }
    mv "$first" "$out/$name/.index"
    find "$out/$name" -maxdepth 1 -type f -name '*.html' -delete
    perl -0pi -e 's/<script\b.*?<\/script\s*>//gis; s/<noscript\b.*?<\/noscript\s*>//gis; s/<meta[^>]*Content-Security-Policy[^>]*>//gi; s/\s(?:integrity|crossorigin)=(?:"[^"]*"|\x27[^\x27]*\x27)//gi; s/<link\b[^>]*rel=["\x27]?(?:preload|modulepreload|prefetch|preconnect|dns-prefetch)[^>]*>//gi' "$out/$name/.index"
    mv "$out/$name/.index" "$out/$name/index.html"
    touch "$out/$name/.done"
    echo "$name $(du -sh "$out/$name" | cut -f1) $(stat -c %s "$out/$name/index.html") bytes, $(ls "$out/$name" | wc -l) files"
done <<< "$pages"
