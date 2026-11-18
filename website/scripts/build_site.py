"""Build the static Bank page at the GitHub Pages project path."""
import argparse
import hashlib
import importlib.util
import json
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
THEME = ROOT / "vendor/theme"
SRC = ROOT / "src"
CANONICAL = "https://volparossa.org/volparossa-bank/"
REPO = "https://github.com/VOLPAROSSA/volparossa-bank"


def render(template, values):
    return re.sub(r"{{(\w+)}}", lambda match: values[match[1]], template)


def arrows(html):
    paths = {"↗": "M3 13 13 3M5 3h8v8", "→": "M2 8h12m-5-5 5 5-5 5"}
    for char, path in paths.items():
        html = html.replace(char, f'<svg class="direction" width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="{path}"/></svg>')
    return html


def theme_values():
    inventory = json.loads((THEME / "theme-version.json").read_text())
    for name, expected in inventory["files"].items():
        if hashlib.sha256((THEME / name).read_bytes()).hexdigest() != expected:
            raise ValueError(f"Modified shared theme file: {name}")
    spec = importlib.util.spec_from_file_location("navigation", THEME / "shared/navigation.py")
    navigation = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(navigation)
    nav = json.loads((THEME / "data/navigation.json").read_text())
    # Resolve the project’s own entry in the pinned shared navigation.
    for item in nav["main"]:
        for app in item.get("children", []):
            if app["name"] == "Bank":
                app["url"] = CANONICAL
    projects = json.loads((THEME / "data/projects.json").read_text())
    links = navigation.site_links(nav, projects, "https://volparossa.org/")
    icon = (THEME / "assets/icons/github.svg").read_text().replace("<svg ", '<svg aria-hidden="true" class="github-icon" ', 1)
    links["nav_links"] = navigation.navigation_links(nav["main"], links, "", icon)
    links["footer_links"] = navigation.navigation_links(nav["footer"], links, "", icon, footer=True)
    return links


def illustration(name, widths, height, sizes, mobile=False):
    base = f"assets/bank/{name}"
    def sources(prefix, ws):
        return ", ".join(f"{prefix}-{w}.webp {w}w" for w in ws)
    crop = ""
    if mobile:
        crop = (f'<source media="(max-width: 700px)" type="image/webp" '
                f'srcset="{sources(base + "-mobile", [480, 800])}" sizes="100vw">'
                f'<source media="(max-width: 700px)" srcset="{base}-mobile.jpg">')
        crop += (f'<source media="(max-width: 900px)" type="image/webp" '
                 f'srcset="{sources(base + "-tablet", [600, 960])}" sizes="54vw">'
                 f'<source media="(max-width: 900px)" srcset="{base}-tablet.jpg">')
    load = 'loading="eager" fetchpriority="high"' if mobile else 'loading="lazy"'
    return (f'<picture>{crop}<source type="image/webp" srcset="{sources(base, widths)}" sizes="{sizes}">'
            f'<img src="{base}.jpg" width="{widths[-1]}" height="{height}" alt="" {load} decoding="async"></picture>')


def components():
    art = json.loads((SRC / "art.json").read_text())
    return {name: illustration(name, **data) for name, data in art.items()}


def build(output, production):
    output.mkdir(parents=True, exist_ok=True)
    marker = output / ".bank-site-build"
    if any(output.iterdir()):
        if not marker.is_file():
            raise ValueError("Choose an empty directory or an existing Bank site build.")
        shutil.rmtree(output)
        output.mkdir()
    marker.touch()
    (output / ".nojekyll").touch()
    shutil.copytree(ROOT / "assets", output / "assets")
    for name in ["site.css", "tokens.css", "site.js", "logo.png", "favicon.png"]:
        shutil.copy2(THEME / "assets" / name, output / "assets" / name)
    for name in ["fonts", "icons"]:
        shutil.copytree(THEME / "assets" / name, output / "assets" / name)
    shutil.copy2(THEME / "favicon.ico", output)
    shutil.copy2(SRC / "bank.css", output / "assets")
    robots = "index,follow" if production else "noindex,nofollow"
    values = {**theme_values(), **components(), "title": "VOLPAROSSA Bank | Portfolio and payment research",
              "description": "Explore a shared portfolio method and a local payment test with fictitious assets and TEST units.",
              "root": "", "canonical": CANONICAL, "body_class": "bank-page",
              "social_image": CANONICAL + "assets/bank/workshop-social.jpg",
              "social_alt": "A glass-enclosed precision balance and an open box of brass calibration weights on the workshop bench.",
              "head_extra": f'<meta name="robots" content="{robots}"><link rel="stylesheet" href="assets/bank.css">',
              "repository": REPO, "research_url": REPO + "/blob/main/docs/RESEARCH.md",
              "test_url": REPO + "/blob/main/docs/PAPER_PAYMENTS.md",
              "core_url": "https://github.com/VOLPAROSSA/volparossa/blob/main/docs/services/TRANSACTION_LAYER.md"}
    for part in ["header", "footer"]:
        values[part] = render((THEME / f"templates/{part}.html").read_text(), values)
    values["content"] = render((SRC / "index.html").read_text(), values)
    (output / "index.html").write_text(arrows(render((THEME / "templates/layout.html").read_text(), values)))
    if production:
        (output / "sitemap.xml").write_text(f'<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>{CANONICAL}</loc></url></urlset>\n')
    print("Built", output)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "preview/volparossa-bank")
    parser.add_argument("--production", action="store_true")
    args = parser.parse_args()
    build(args.output.resolve(), args.production)
