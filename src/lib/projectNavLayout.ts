import { useEffect, useState } from "react";

export type ProjectNavLayout = "horizontal" | "sidebar";

const STORAGE_KEY = "automatic.project_nav_layout";
const CHANGE_EVENT = "automatic:project_nav_layout";
const DEFAULT_LAYOUT: ProjectNavLayout = "sidebar";

function readFromStorage(): ProjectNavLayout {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === "sidebar" || raw === "horizontal") return raw;
  } catch {
    // localStorage may throw in restricted contexts; fall through to default
  }
  return DEFAULT_LAYOUT;
}

export function getProjectNavLayout(): ProjectNavLayout {
  return readFromStorage();
}

export function setProjectNavLayout(value: ProjectNavLayout): void {
  try {
    localStorage.setItem(STORAGE_KEY, value);
  } catch {
    // ignore
  }
  window.dispatchEvent(new CustomEvent(CHANGE_EVENT, { detail: value }));
}

export function useProjectNavLayout(): ProjectNavLayout {
  const [layout, setLayout] = useState<ProjectNavLayout>(() => readFromStorage());

  useEffect(() => {
    const onCustom = () => setLayout(readFromStorage());
    const onStorage = (e: StorageEvent) => {
      if (e.key === STORAGE_KEY) setLayout(readFromStorage());
    };
    window.addEventListener(CHANGE_EVENT, onCustom);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(CHANGE_EVENT, onCustom);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  return layout;
}
