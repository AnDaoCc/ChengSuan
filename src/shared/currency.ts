const chineseCurrencyNames = new Intl.DisplayNames(['zh-CN'], {
  type: 'currency',
  fallback: 'none',
});

/** Localized using built-in locale data; no network request is needed. */
export function currencyLabel(code: string): string {
  const name = chineseCurrencyNames.of(code);
  return name && name !== code ? `${code} · ${name}` : code;
}
