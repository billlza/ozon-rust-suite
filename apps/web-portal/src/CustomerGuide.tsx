import { Orbit } from "lucide-react";
import type { ReactNode } from "react";
import { useI18n } from "./i18n";
import { LanguageSwitch } from "./LanguageSwitch";

type GuideStepCopy = {
  title: string;
  paragraphs: string[];
  items?: string[];
  action?: string;
};

export function CustomerGuide() {
  const { copy, locale, setLocale } = useI18n();
  const guide = copy.guide;

  return (
    <main className="customer-guide">
      <header className="guide-topbar">
        <a className="brand-mark" href="/" aria-label={copy.common.brand}>
          <span className="brand-icon">
            <Orbit size={18} />
          </span>
          <span>{copy.common.brand}</span>
        </a>
        <div className="guide-top-actions">
          <LanguageSwitch locale={locale} setLocale={setLocale} />
          <a className="guide-return" href="/">
            {guide.returnToPortal}
          </a>
        </div>
      </header>

      <section className="guide-hero" id="top">
        <p className="guide-eyebrow">{guide.hero.eyebrow}</p>
        <h1>{guide.hero.title}</h1>
        <p>{guide.hero.text}</p>
      </section>

      <div className="guide-layout">
        <aside className="guide-side" aria-label={guide.sideAria}>
          <strong>{guide.quickJump}</strong>
          {guide.navItems.map((item) => (
            <a href={item.href} key={item.href}>
              {item.label}
            </a>
          ))}
        </aside>

        <div className="guide-content">
          <section className="guide-section" id="prepare">
            <h2>{guide.prepare.title}</h2>
            <div className="guide-list">
              {guide.prepare.items.map((item) => (
                <GuideItem key={item.label} label={item.label} text={item.text} />
              ))}
            </div>
          </section>

          <section className="guide-section" id="setup">
            <h2>{guide.setup.title}</h2>
            <div className="guide-steps">
              {guide.setup.steps.map((step, index) => (
                <GuideCopyStep index={index + 1} key={step.title} step={step} />
              ))}
            </div>
          </section>

          <section className="guide-section" id="work">
            <h2>{guide.work.title}</h2>
            <div className="guide-steps">
              {guide.work.steps.map((step, index) => (
                <GuideCopyStep index={index + 1} key={step.title} step={step} />
              ))}
            </div>
          </section>

          <section className="guide-section guide-note" id="daily">
            <h2>{guide.daily.title}</h2>
            <p>{guide.daily.intro}</p>
            <ul>
              {guide.daily.items.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
            <p>{guide.daily.outro}</p>
          </section>

          <section className="guide-section" id="faq">
            <h2>{guide.faq.title}</h2>
            <div className="guide-list">
              {guide.faq.items.map((item) => (
                <GuideQuestion key={item.title} title={item.title} text={item.text} />
              ))}
            </div>
          </section>

          <section className="guide-section guide-warning" id="support">
            <h2>{guide.support.title}</h2>
            <p>{guide.support.intro}</p>
            <ul>
              {guide.support.items.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
            <p>{guide.support.outro}</p>
          </section>
        </div>
      </div>

      <footer className="guide-footer">{guide.footer}</footer>
    </main>
  );
}

function GuideCopyStep({ index, step }: { index: number; step: GuideStepCopy }) {
  return (
    <GuideStep index={index} title={step.title}>
      {step.paragraphs.map((paragraph) => (
        <p key={paragraph}>{paragraph}</p>
      ))}
      {step.items && (
        <ul>
          {step.items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      )}
      {step.action && (
        <div className="guide-actions">
          <a className="guide-button" href="/">
            {step.action}
          </a>
        </div>
      )}
    </GuideStep>
  );
}

function GuideItem({ label, text }: { label: string; text: string }) {
  return (
    <div className="guide-item">
      <span>{label}</span>
      <p>{text}</p>
    </div>
  );
}

function GuideQuestion({ title, text }: { title: string; text: string }) {
  return (
    <div className="guide-item">
      <h3>{title}</h3>
      <p>{text}</p>
    </div>
  );
}

function GuideStep({
  children,
  index,
  title
}: {
  children: ReactNode;
  index: number;
  title: string;
}) {
  return (
    <div className="guide-step">
      <span className="guide-number">{index}</span>
      <div>
        <h3>{title}</h3>
        {children}
      </div>
    </div>
  );
}
