// Free-form target/candidate entry: element/amount rows only -- no
// formula-string parser (e.g. typing "BaTiO3" and having it expand to
// Ba:1/Ti:1/O:3). gugen itself has no formula-string parser anywhere in
// its public API (`Composition` takes explicit (Element, amount) pairs;
// `commercial_catalog.rs`'s own internal parser is private and feature-
// gated, not exposed to this wrapper) -- building one here would mean
// inventing new chemistry-parsing logic outside gugen's own core, which
// risks disagreeing with its semantics. Row-based entry reuses the exact
// same request shape plan_synthesis already accepts, so this needed zero
// new WASM/Rust code, matching every prior playground phase.

// Mirrors playground/wasm/src/lib.rs's own limits -- checked again
// server-side regardless (the WASM wrapper is the real trust boundary),
// this is purely for immediate client-side feedback.
const MAX_TARGET_ELEMENTS = 12;
const MAX_CANDIDATES = 60;
const MAX_SYMBOL_LEN = 40;

function el(tag, opts = {}) {
  const node = document.createElement(tag);
  if (opts.className) node.className = opts.className;
  if (opts.type) node.type = opts.type;
  if (opts.text !== undefined) node.textContent = opts.text;
  if (opts.placeholder !== undefined) node.placeholder = opts.placeholder;
  if (opts.ariaLabel !== undefined) node.setAttribute("aria-label", opts.ariaLabel);
  return node;
}

function elementRow(onRemove) {
  const row = el("div", { className: "element-row" });
  const symbol = el("input", { type: "text", className: "element-symbol", ariaLabel: "Element symbol" });
  symbol.placeholder = "Symbol";
  const amount = el("input", { type: "number", className: "element-amount", ariaLabel: "Amount" });
  amount.placeholder = "Amount";
  amount.min = "0";
  amount.step = "any";
  const remove = el("button", { type: "button", className: "remove-row-button", ariaLabel: "Remove this element" });
  remove.textContent = "×";
  remove.addEventListener("click", () => {
    row.remove();
    onRemove?.();
  });
  row.append(symbol, amount, remove);
  return row;
}

function readElementRows(container) {
  const elements = {};
  for (const row of container.querySelectorAll(".element-row")) {
    const symbol = row.querySelector(".element-symbol").value.trim();
    const amountText = row.querySelector(".element-amount").value.trim();
    if (!symbol && !amountText) continue;
    if (!symbol) throw new Error("An element row has an amount but no symbol.");
    if (symbol.length > MAX_SYMBOL_LEN) {
      throw new Error(`Element symbol "${symbol}" is too long (max ${MAX_SYMBOL_LEN} characters).`);
    }
    const amount = Number(amountText);
    if (!amountText || !Number.isFinite(amount) || amount <= 0) {
      throw new Error(`Element "${symbol}" needs a positive amount.`);
    }
    if (symbol in elements) {
      throw new Error(`Element "${symbol}" was entered more than once.`);
    }
    elements[symbol] = amount;
  }
  return elements;
}

function candidateRow(onRemove) {
  const row = el("div", { className: "candidate-row" });
  const idLabel = el("label", { className: "candidate-id-label" });
  idLabel.textContent = "Precursor id ";
  const id = el("input", { type: "text", className: "candidate-id", ariaLabel: "Precursor id" });
  id.placeholder = "e.g. BaCO3";
  idLabel.appendChild(id);

  const elementsContainer = el("div", { className: "element-rows" });
  const addElementButton = el("button", { type: "button", className: "add-row-button", text: "+ Add element" });
  addElementButton.addEventListener("click", () => {
    elementsContainer.appendChild(elementRow());
  });

  const removeButton = el("button", { type: "button", className: "remove-row-button remove-candidate", text: "Remove candidate" });
  removeButton.addEventListener("click", () => {
    row.remove();
    onRemove?.();
  });

  row.append(idLabel, elementsContainer, addElementButton, removeButton);
  elementsContainer.appendChild(elementRow());
  return row;
}

function readCandidateRows(container) {
  const candidates = [];
  const seenIds = new Set();
  for (const row of container.querySelectorAll(".candidate-row")) {
    const id = row.querySelector(".candidate-id").value.trim();
    const elementsContainer = row.querySelector(".element-rows");
    const elements = readElementRows(elementsContainer);
    if (!id && Object.keys(elements).length === 0) continue;
    if (!id) throw new Error("A candidate row has elements but no precursor id.");
    if (id.length > MAX_SYMBOL_LEN) {
      throw new Error(`Precursor id "${id}" is too long (max ${MAX_SYMBOL_LEN} characters).`);
    }
    if (seenIds.has(id)) {
      throw new Error(`Precursor id "${id}" was entered more than once.`);
    }
    if (Object.keys(elements).length === 0) {
      throw new Error(`Candidate "${id}" needs at least one element.`);
    }
    seenIds.add(id);
    candidates.push({ id, elements });
  }
  if (candidates.length > MAX_CANDIDATES) {
    throw new Error(`${candidates.length} candidates entered, exceeding the ${MAX_CANDIDATES}-candidate limit.`);
  }
  return candidates;
}

export function initCustomTarget() {
  const targetContainer = document.getElementById("custom-target-elements");
  const candidatesContainer = document.getElementById("custom-candidates");

  targetContainer.appendChild(elementRow());
  candidatesContainer.appendChild(candidateRow());

  document.getElementById("add-target-element").addEventListener("click", () => {
    targetContainer.appendChild(elementRow());
  });
  document.getElementById("add-candidate").addEventListener("click", () => {
    candidatesContainer.appendChild(candidateRow());
  });
}

// Throws Error with a user-facing message on any validation problem --
// callers should catch and show it via the same status/error surface
// the curated-example flow already uses.
export function readCustomRequest() {
  const targetContainer = document.getElementById("custom-target-elements");
  const candidatesContainer = document.getElementById("custom-candidates");

  const target_elements = readElementRows(targetContainer);
  const elementCount = Object.keys(target_elements).length;
  if (elementCount === 0) {
    throw new Error("Enter at least one target element.");
  }
  if (elementCount > MAX_TARGET_ELEMENTS) {
    throw new Error(`${elementCount} target elements entered, exceeding the ${MAX_TARGET_ELEMENTS}-element limit.`);
  }

  const candidates = readCandidateRows(candidatesContainer);
  if (candidates.length === 0) {
    throw new Error("Enter at least one precursor candidate.");
  }

  return { target_elements, candidates };
}
