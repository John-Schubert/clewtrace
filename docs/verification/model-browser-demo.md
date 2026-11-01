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
- [ ] With the text editor: open a `.sysml` file, add a part, and pause for a moment. The
      tree shows the new part without closing the open packages, and the selected row stays
      selected. Task 2.9b built this; the unit tests cover it, and it has not been demonstrated
      by hand in the running app yet.
- [ ] Autosave: type in the editor and stop. Within about a second the status says Saved and
      the file on disk has the edit. Type again and click the model browser: the edit is
      saved. Type again and close the window at once: it closes, and the file has the edit.
      Task 2.10a built this; the unit tests cover it, and it has not been demonstrated by
      hand in the running app yet.

## Result

Not yet run on a built app. Run on Windows, macOS, and Linux at the next pre-release.
