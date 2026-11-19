import clsx from "clsx";
import { openExternalLink } from "../../utils/externalLinks";
import { TruncatedText } from "../foundation/TruncatedText";

export function InlineMeta({
  href,
  label,
  value,
  mono = false,
}: {
  href?: string;
  label: string;
  value: string;
  mono?: boolean;
}) {
  const valueClassName = clsx(
    "text-body-sm font-semibold text-on-surface/90",
    mono && "font-mono text-primary",
    href &&
      "hover:text-primary-strong hover:underline hover:decoration-primary/55 hover:underline-offset-2",
  );

  return (
    <div
      className={clsx(
        "flex min-w-0 items-baseline gap-2",
        mono ? "max-w-60 shrink-0" : "max-w-[560px] flex-1",
      )}
    >
      <span className="shrink-0 text-label-caps uppercase text-outline/90">
        {label}
      </span>
      {href ? (
        <TruncatedText
          as="a"
          className={valueClassName}
          href={href}
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            void openExternalLink(href);
          }}
          rel="noreferrer"
          target="_blank"
        >
          {value}
        </TruncatedText>
      ) : (
        <TruncatedText className={valueClassName}>{value}</TruncatedText>
      )}
    </div>
  );
}
