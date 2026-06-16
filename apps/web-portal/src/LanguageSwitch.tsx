import { messages, type Locale } from "./i18n";

export function LanguageSwitch({
  locale,
  setLocale
}: {
  locale: Locale;
  setLocale: (locale: Locale) => void;
}) {
  const labels = messages[locale].common.language;

  return (
    <div className="language-switch" aria-label={labels.ariaLabel}>
      {(["zh", "en"] as const).map((nextLocale) => (
        <button
          aria-pressed={locale === nextLocale}
          className={locale === nextLocale ? "active" : ""}
          key={nextLocale}
          onClick={() => setLocale(nextLocale)}
          type="button"
        >
          {labels[nextLocale]}
        </button>
      ))}
    </div>
  );
}
