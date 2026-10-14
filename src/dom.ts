// Pequeñas utilidades compartidas por las dos pantallas.

export function el(id: string): HTMLElement {
  const found = document.getElementById(id);
  if (!found) throw new Error(`Falta el elemento #${id}`);
  return found;
}

export function li(className: string, text: string): HTMLLIElement {
  const item = document.createElement("li");
  item.className = className;
  item.textContent = text;
  return item;
}

/// Cambia entre la pantalla principal y la de Ajustes.
export function showView(view: "main" | "settings") {
  el("view-main").hidden = view !== "main";
  el("view-settings").hidden = view !== "settings";
}
