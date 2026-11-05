export function remember(key: string, value: string) {
  try {
    localStorage.setItem(`stackyard.${key}`, value);
  } catch {
    // A restricted WebView profile can still use navigation for the current session.
  }
}

export function recalled(key: string): string | null {
  try {
    return localStorage.getItem(`stackyard.${key}`);
  } catch {
    return null;
  }
}
