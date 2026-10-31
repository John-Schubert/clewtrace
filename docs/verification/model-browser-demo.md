<!-- @verifies CT-MOD-005 -->
# Model browser demonstration

Verification method: Demonstration. Repeat before each release and whenever the model
browser, the detail view, or the problems panel changes. Record the result below.

## Checklist

- [ ] Open a project folder. The tree lists its top-level packages and definitions.
- [ ] The right arrow (or a click on the triangle) opens a package; the left arrow closes it,
      or moves to its parent when it is already closed.
- [ ] A package with more than 200 members scrolls smoothly and fills in as you scroll.
- [ ] Selecting an element shows its kind, qualified name, doc, attributes, and file and line.
- [ ] The problems panel lists each error, then each warning, as file:line:column with its
      fix hint. Selecting one shows where, what went wrong, and how to fix it.
- [ ] With the text editor (a later task): editing the text updates the tree as you type.
      This item cannot be run until the editor exists.

## Result

Not yet run on a built app. Run on Windows, macOS, and Linux at the next pre-release.
