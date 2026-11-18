# VOLPAROSSA Bank project website

The English research page is built for https://volparossa.org/volparossa-bank/.
Its first example calculates illustrative target weights. Its second follows one
owner-signed instruction through a local restart and retry with the real Core
library. The page is static and has no financial data entry, execution or analytics.

## Build and review

Python 3 builds the website from a clean checkout. Run from the repository root.

```sh
python3 website/scripts/build_site.py
python3 website/scripts/check_site.py
python3 -m http.server 8798 --bind 127.0.0.1 --directory website/preview
```

Open http://127.0.0.1:8798/volparossa-bank/. The preview is marked noindex.
For the GitHub Pages artifact use the following commands.

```sh
python3 website/scripts/build_site.py --production --output website/dist/volparossa-bank
python3 website/scripts/check_site.py --production --output website/dist/volparossa-bank
```

The Bank website workflow preserves the review build as a downloadable pull
request artifact. On main, it checks and deploys the production artifact using
GitHub Actions. Shared brand assets, fonts and section artwork are served locally.

## Browser checks

With the preview server running, Node 24 and Chromium installed, run the following
in another terminal. Set CHROME_PATH to the browser executable on your system.

```sh
npm --prefix website ci --ignore-scripts
CHROME_PATH=/usr/bin/chromium npm --prefix website run qa
```

The runner also accepts QA_ENGINES=firefox,webkit with the corresponding
Playwright browsers installed. QA_URL overrides the preview address.

[Desktop render](review/desktop.webp) · [Mobile render](review/mobile.webp)

- Compared the rendered desktop to the supplied counting-house reference. The opening
  composition retains the gabled window, vase, quill, ledger and two-pan balance.
  The names and weights sit on separate leaves of the open book. Three pinned
  documents precede the darker payment test. A facade engraving, research ledgers
  and a brass hand stamp finish the page.
- Checked 1440, 768, 390 and 320 CSS pixels, the skip link, keyboard disclosure,
  mobile menu, desktop hover, application links, direct anchor reloads, local
  font and image loading, the native UI without scripts or images, and 200% text.
  Chromium runs the automated WCAG A/AA checks with the calculation expanded.
- Checked the rendered company scores (0.006, 0.004, 0) against their inputs,
  a total score of 0.010 and target weights of 60%, 40% and 0%. Checked the
  100-TEST total in each state and identical 30/70 balances on retry.

## Sources and artwork

The public research scope, inputs and payment sequence were checked against
[Bank commit a7b9b65](https://github.com/VOLPAROSSA/volparossa-bank/tree/a7b9b65a3ab2cf3d6df195c61e26cebf3017b559),
including AGENTS.md, README.md, docs/RESEARCH.md and docs/PAPER_PAYMENTS.md. Visitors
can reach the current method, test record, execution instructions and Core
transaction-layer documentation from the corresponding section.

The header, footer, menu behavior, brand mark, favicon, fonts and design tokens
are the unmodified source-only export of central theme 1.1.0 from
[commit f48c575](https://github.com/VOLPAROSSA/volparossa.github.io/tree/f48c575c49d196dca68e6129e785294aa704c5cf).
The builder verifies the export checksums. The [theme notices](vendor/theme/notices),
font licences, GitHub icon licence and repository GPL-3.0-only licence are retained.

The project owner supplied the design reference “Concept 01 — De rekenkamer”.
The seven decorative subjects in assets/bank are new AI-generated illustrations
created for VOLPAROSSA from that reference. The original concept is not embedded
or redistributed. The paper and oak backgrounds are material samples from the
generated instruction and stacked-books images. Local WebP variants, JPEG
fallbacks, the mobile crop and the 1200 × 630 social image are derivatives of
that same artwork. The small outline symbols in src/icons were drawn for this
page. These original website materials are provided with the repository under
its licence to the extent rights apply. The VOLPAROSSA mark and third-party
materials retain the terms in the shared notices.
