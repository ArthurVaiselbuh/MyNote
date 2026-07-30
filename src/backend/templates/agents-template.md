# MyNote notebook — guide for AI agents

This folder is a MyNote notebook: plain Markdown pages plus one JSON index.

**DO NOT ADD/MODIFY/DELETE PLACE ANYTHING INTO THIS FOLDER — no edits, no new files, no scratch scripts**.

This guide exists so agents can read and understand the notebook's structure without ruining it.

## Layout

- `notebook.json` — section list and nested page tree. Each page node has
  `id`, `title`, `expanded`, `children`. `notebook.json.bak` is a
  crash-recovery copy.
- `<uuid>.md` — one file per page, named by its page id from `notebook.json`.
  The first non-empty line is the page title as an H1 (`# Title`).
- `assets/<page-id>/` — images pasted into that page, referenced from its
  Markdown as `assets/<page-id>/<file>`.
- `.git` - if git is available on the machine then the notebook has a full git history which you may search with git cli if the user requests it.

## Listing the pages

To map page titles to their files, run this from the notebook root (without saving it into this folder):

```python
import json
from pathlib import Path

def walk(pages, depth=1):
    for page in pages:
        print(f"{'    ' * depth}{page['title']}  [{page['id']}.md]")
        walk(page.get("children", []), depth + 1)

notebook = json.loads(Path("notebook.json").read_text(encoding="utf-8"))
for section in notebook["sections"]:
    print(section["name"])
    walk(section["pages"])
```

Output: one line per section, its pages indented beneath it, each with the `.md` file that holds that page's content.

## Searching

To search for something - use text search tools rather than reading all pages, then read relevant ones.
Once you have an understanding of the relevant pages - check where they fit in the hierarchy of notebook.json .
Read the titles of sibling/child/parent pages and consider reading their content even if they don't directly match your search since they might contain relevant data.
