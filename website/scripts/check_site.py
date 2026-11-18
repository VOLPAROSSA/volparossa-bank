"""Validate the generated document, its local assets and both worked examples."""
import argparse
from decimal import Decimal
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlsplit


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.tables = []
        self.table = None
        self.cell = None
        self.rows = []
        self.ids = set()
        self.links = []
        self.assets = []
        self.h1 = self.sections = 0
        self.images = []
        self.cell_key = False
        self.meta = {}

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            assert a["id"] not in self.ids, a["id"]
            self.ids.add(a["id"])
        if tag == "a":
            self.links.append(a["href"])
        if tag in ("link", "script", "img"):
            self.assets.append(a.get("href") or a.get("src"))
        if tag == "source":
            self.assets.extend(item.strip().split()[0] for item in a["srcset"].split(","))
        if tag == "span" and a.get("class") == "cell-key":
            self.cell_key = True
        if tag == "img":
            self.images.append(a)
        if tag == "h1":
            self.h1 += 1
        if tag == "section":
            self.sections += 1
        if tag == "meta":
            self.meta[a.get("name") or a.get("property")] = a.get("content")
        if tag == "table":
            self.table = []
            self.tables.append(self.table)
        if tag == "tr":
            self.rows = []
            self.table.append(self.rows)
        if tag in ("td", "th"):
            if tag == "th":
                assert a.get("scope") in ("row", "col")
            self.cell = ""

    def handle_data(self, text):
        if self.cell is not None and not self.cell_key:
            self.cell += text

    def handle_endtag(self, tag):
        if tag == "span":
            self.cell_key = False
        if tag in ("td", "th"):
            self.rows.append(self.cell.strip())
            self.cell = None
        if tag == "table":
            self.table = None


def check(folder, production):
    html = (folder / "index.html").read_text()
    page = Page()
    page.feed(html)
    assert page.sections == 7 and page.h1 == 1 and html.count("<caption>") == 3
    assert "{{" not in html and '<html lang="en">' in html
    assert page.meta["robots"] == ("index,follow" if production else "noindex,nofollow")
    assert page.meta["og:url"] == "https://volparossa.org/volparossa-bank/"
    for link in page.links:
        if link.startswith("#"):
            assert link[1:] in page.ids
        else:
            assert urlsplit(link).scheme == "https", link
    for asset in page.assets:
        if not urlsplit(asset).netloc:
            assert not asset.startswith("/") and (folder / asset).is_file(), asset
    for im in page.images:
        assert all(attr in im for attr in ("alt", "width", "height"))
    assert (folder / "assets/bank/social.jpg").is_file()
    for n in ("paper", "oak"):
        assert (folder / f"assets/bank/{n}.webp").is_file()
    assert len(page.tables) == 3
    book, score, balances = [t[1:] for t in page.tables]
    percent = lambda text: Decimal(text.removesuffix("%")) / 100
    scores = [percent(row[1]) * percent(row[2]) for row in score]
    total = sum(scores)
    assert total == Decimal("0.010")
    for row, calculated in zip(score, scores):
        assert Decimal(row[3]) == calculated and percent(row[4]) == calculated / total
    assert book == [[row[0], row[4]] for row in score if Decimal(row[3]) > 0]
    assert score[-1][4] == "0%" and sum(percent(r[4]) for r in score) == 1
    assert balances == [["Start", "100 TEST", "0 TEST"], ["After transfer", "30 TEST", "70 TEST"],
                        ["After restart and retry", "30 TEST", "70 TEST"]]
    assert all(sum(int(v.split()[0]) for v in row[1:]) == 100 for row in balances)
    assert "<dd>70 TEST</dd>" in html
    print("Verified seven sections, accessible tables, local paths, target weights and restart balances.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parents[1] / "preview/volparossa-bank")
    parser.add_argument("--production", action="store_true")
    args = parser.parse_args()
    check(args.output, args.production)
