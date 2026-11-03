// The one tooltip every control uses, as a Svelte action: `use:tip={{ name, shortcut }}`.
// It shows on hover and keyboard focus, closes with Esc, and links to the control with
// aria-describedby while shown. No title attributes anywhere.

// @implements CT-USE-010
export interface TipSpec {
  name: string;
  /** A key hint such as `Ctrl+S`. */
  shortcut?: string;
}

// @implements CT-USE-010
/** What the tooltip says: the name, then the shortcut in brackets. */
export function tipText(spec: TipSpec | string): string {
  const { name, shortcut } = typeof spec === 'string' ? { name: spec, shortcut: undefined } : spec;
  return shortcut ? `${name} (${shortcut})` : name;
}

// @begin CT-USE-010
const TIP_ID = 'clew-tooltip';
let element: HTMLElement | undefined;
let owner: HTMLElement | undefined;
let listening = false;

function shared(): HTMLElement {
  if (!element || !element.isConnected) {
    element = document.createElement('div');
    element.id = TIP_ID;
    element.setAttribute('role', 'tooltip');
    element.hidden = true;
    element.style.cssText =
      'position:fixed;z-index:1000;max-width:20rem;padding:0.25rem 0.5rem;border-radius:4px;' +
      'background:var(--text);color:var(--bg);font-size:0.85rem;pointer-events:none;';
    document.body.append(element);
  }
  if (!listening) {
    listening = true;
    // Esc closes the tooltip wherever focus is (WCAG 1.4.13).
    document.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        hide();
      }
    });
  }
  return element;
}

function hide(from?: HTMLElement): void {
  if (from && owner !== from) {
    return;
  }
  owner?.removeAttribute('aria-describedby');
  owner = undefined;
  if (element) {
    element.hidden = true;
  }
}

function show(node: HTMLElement, spec: TipSpec | string): void {
  hide();
  const tipEl = shared();
  tipEl.textContent = tipText(spec);
  tipEl.hidden = false;
  const box = node.getBoundingClientRect();
  const width = tipEl.offsetWidth;
  const left = Math.max(4, Math.min(box.left, window.innerWidth - width - 4));
  tipEl.style.left = `${left}px`;
  tipEl.style.top = `${box.bottom + 4}px`;
  node.setAttribute('aria-describedby', TIP_ID);
  owner = node;
}

/** Svelte action: gives a control the shared tooltip. */
export function tip(node: HTMLElement, initial: TipSpec | string): { update: (spec: TipSpec | string) => void; destroy: () => void } {
  let spec = initial;
  const on = () => show(node, spec);
  const off = () => hide(node);
  node.addEventListener('mouseenter', on);
  node.addEventListener('mouseleave', off);
  node.addEventListener('focus', on);
  node.addEventListener('blur', off);
  return {
    update(next) {
      spec = next;
      if (owner === node) {
        show(node, spec);
      }
    },
    destroy() {
      node.removeEventListener('mouseenter', on);
      node.removeEventListener('mouseleave', off);
      node.removeEventListener('focus', on);
      node.removeEventListener('blur', off);
      hide(node);
    },
  };
}
// @end
