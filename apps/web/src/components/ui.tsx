import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { CloudOff, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  type ButtonHTMLAttributes,
  forwardRef,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import { useTranslation } from "react-i18next";

export function cx(...classes: (string | false | null | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

type Variant = "primary" | "secondary" | "ghost" | "danger";

const variants: Record<Variant, string> = {
  primary:
    "bg-accent text-accent-ink hover:bg-accent-strong shadow-[0_8px_30px_-12px] shadow-accent/60",
  secondary: "bg-white/10 text-fg hover:bg-white/16 backdrop-blur",
  ghost: "text-muted hover:text-fg hover:bg-white/6",
  danger: "bg-danger/15 text-danger hover:bg-danger/25",
};

const sizes = {
  sm: "h-8 px-3 text-sm gap-1.5",
  md: "h-10 px-4 text-sm gap-2",
  lg: "h-12 px-6 text-base gap-2.5",
};

type ButtonLook = { variant?: Variant; size?: keyof typeof sizes; className?: string };

/** Button looks, also worn by links that navigate: one focusable element, not a button in a link. */
export function buttonClass({ variant = "secondary", size = "md", className }: ButtonLook = {}) {
  return cx(
    "inline-flex items-center justify-center rounded-full font-semibold transition-all duration-200",
    "active:scale-[0.97] disabled:opacity-50 disabled:pointer-events-none cursor-pointer select-none",
    variants[variant],
    sizes[size],
    className,
  );
}

export function iconButtonClass(className?: string) {
  return cx(
    "inline-flex size-10 items-center justify-center rounded-full text-fg/90 transition-all",
    "hover:bg-white/10 hover:text-fg active:scale-90 cursor-pointer disabled:opacity-40",
    className,
  );
}

export const Button = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & ButtonLook & { loading?: boolean }
>(function Button({ variant, size, loading, className, children, disabled, ...props }, ref) {
  return (
    <button
      ref={ref}
      disabled={disabled || loading}
      className={buttonClass({ variant, size, className })}
      {...props}
    >
      {loading ? <Spinner className="size-4" /> : null}
      {children}
    </button>
  );
});

export function IconButton({
  label,
  className,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={iconButtonClass(className)}
      {...props}
    >
      {children}
    </button>
  );
}

export function Spinner({ className }: { className?: string }) {
  return (
    <span
      role="status"
      className={cx(
        "inline-block animate-spin rounded-full border-2 border-current border-t-transparent",
        className ?? "size-6",
      )}
    />
  );
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cx("skeleton rounded-[var(--radius-card)]", className)} />;
}

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  function Input({ className, ...props }, ref) {
    return (
      <input
        ref={ref}
        className={cx(
          "h-11 w-full rounded-xl border border-line bg-surface-2 px-4 text-fg placeholder:text-faint",
          "outline-none transition focus:border-accent/70 focus:bg-surface-3 focus:ring-4 focus:ring-accent/15",
          className,
        )}
        {...props}
      />
    );
  },
);

export function Select({ className, children, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className={cx(
        "h-11 rounded-xl border border-line bg-surface-2 px-3 pr-8 text-sm text-fg outline-none",
        "transition focus:border-accent/70 cursor-pointer",
        className,
      )}
      {...props}
    >
      {children}
    </select>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  // The hint stays outside the <label>: it must not become part of the field's name.
  return (
    <div className="flex flex-col gap-1.5">
      {/* biome-ignore lint/a11y/noLabelWithoutControl: the control is passed as children */}
      <label className="flex flex-col gap-1.5">
        <span className="text-sm font-medium text-muted">{label}</span>
        {children}
      </label>
      {hint ? <span className="text-xs text-faint">{hint}</span> : null}
    </div>
  );
}

export function ErrorText({ children }: { children: ReactNode }) {
  return (
    <AnimatePresence>
      {children ? (
        <motion.p
          role="alert"
          initial={{ opacity: 0, y: -4 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0 }}
          className="rounded-lg bg-danger/10 px-3 py-2 text-sm text-danger"
        >
          {children}
        </motion.p>
      ) : null}
    </AnimatePresence>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "accent" | "success";
}) {
  const tones = {
    neutral: "bg-white/8 text-muted",
    accent: "bg-accent/15 text-accent",
    success: "bg-success/15 text-success",
  };
  return (
    <span className={cx("rounded-md px-2 py-0.5 text-xs font-semibold", tones[tone])}>
      {children}
    </span>
  );
}

export function ProgressBar({ value, className }: { value: number; className?: string }) {
  return (
    <div className={cx("h-1 overflow-hidden rounded-full bg-white/20", className)}>
      <motion.div
        className="h-full rounded-full bg-accent"
        initial={{ width: 0 }}
        animate={{ width: `${Math.round(value * 100)}%` }}
        transition={{ duration: 0.6, ease: "easeOut" }}
      />
    </div>
  );
}

/** Page wrapper: fades and slides in on navigation. */
export function Page({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <motion.main
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.35, ease: [0.22, 1, 0.36, 1] }}
      className={className}
    >
      {children}
    </motion.main>
  );
}

export function EmptyState({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      className="mx-auto flex max-w-md flex-col items-center gap-3 py-24 text-center"
    >
      <div className="flex size-16 items-center justify-center rounded-2xl bg-surface-2 text-accent">
        {icon}
      </div>
      <h2 className="text-lg font-semibold">{title}</h2>
      {children ? <div className="text-sm text-muted">{children}</div> : null}
    </motion.div>
  );
}

/** A query that failed: says so and offers to try again, instead of an empty page. */
export function LoadError({ onRetry }: { onRetry: () => unknown }) {
  const { t } = useTranslation();
  return (
    <EmptyState icon={<CloudOff className="size-8" />} title={t("common.loadError")}>
      <Button variant="primary" className="mt-2" onClick={() => void onRetry()}>
        {t("common.retry")}
      </Button>
    </EmptyState>
  );
}

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  wide,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  wide?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open ? (
          <DialogPrimitive.Portal forceMount>
            <DialogPrimitive.Overlay asChild forceMount>
              <motion.div
                className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
              />
            </DialogPrimitive.Overlay>
            <DialogPrimitive.Content asChild forceMount>
              <motion.div
                className={cx(
                  "fixed left-1/2 top-1/2 z-50 max-h-[85dvh] w-[calc(100vw-2rem)] overflow-y-auto rounded-2xl",
                  "border border-line bg-surface p-6 shadow-2xl",
                  wide ? "max-w-2xl" : "max-w-md",
                )}
                initial={{ opacity: 0, scale: 0.94, x: "-50%", y: "-46%" }}
                animate={{ opacity: 1, scale: 1, x: "-50%", y: "-50%" }}
                exit={{ opacity: 0, scale: 0.96, x: "-50%", y: "-48%" }}
                transition={{ type: "spring", duration: 0.35, bounce: 0.15 }}
              >
                <div className="mb-4 flex items-start justify-between gap-4">
                  <div>
                    <DialogPrimitive.Title className="text-lg font-semibold">
                      {title}
                    </DialogPrimitive.Title>
                    {description ? (
                      <DialogPrimitive.Description className="mt-1 text-sm text-muted">
                        {description}
                      </DialogPrimitive.Description>
                    ) : null}
                  </div>
                  <DialogPrimitive.Close asChild>
                    <IconButton label={t("common.close")} className="-mr-2 -mt-2 size-8">
                      <X className="size-4" />
                    </IconButton>
                  </DialogPrimitive.Close>
                </div>
                {children}
              </motion.div>
            </DialogPrimitive.Content>
          </DialogPrimitive.Portal>
        ) : null}
      </AnimatePresence>
    </DialogPrimitive.Root>
  );
}

export const MenuRoot = Menu.Root;
export const MenuTrigger = Menu.Trigger;

export function MenuContent({
  children,
  align = "end",
}: {
  children: ReactNode;
  align?: "start" | "end";
}) {
  return (
    <Menu.Portal>
      <Menu.Content
        align={align}
        sideOffset={8}
        className={cx(
          "z-50 min-w-48 rounded-xl border border-line bg-surface-2/95 p-1.5 shadow-2xl backdrop-blur-xl",
          "origin-[var(--radix-dropdown-menu-content-transform-origin)]",
          "data-[state=open]:animate-[menu-in_160ms_ease-out]",
        )}
      >
        {children}
      </Menu.Content>
    </Menu.Portal>
  );
}

export function MenuItem({
  children,
  onSelect,
  danger,
}: {
  children: ReactNode;
  onSelect?: () => void;
  danger?: boolean;
}) {
  return (
    <Menu.Item
      onSelect={onSelect}
      className={cx(
        "flex cursor-pointer items-center gap-2.5 rounded-lg px-3 py-2 text-sm outline-none transition-colors",
        danger ? "text-danger data-[highlighted]:bg-danger/10" : "data-[highlighted]:bg-white/8",
      )}
    >
      {children}
    </Menu.Item>
  );
}

export const MenuSeparator = () => <Menu.Separator className="my-1 h-px bg-line" />;

export function Tabs({
  value,
  onValueChange,
  tabs,
  children,
}: {
  value: string;
  onValueChange: (value: string) => void;
  tabs: { value: string; label: string }[];
  children: ReactNode;
}) {
  return (
    <TabsPrimitive.Root value={value} onValueChange={onValueChange}>
      <TabsPrimitive.List className="no-scrollbar mb-8 flex gap-1 overflow-x-auto border-b border-line">
        {tabs.map((tab) => (
          <TabsPrimitive.Trigger
            key={tab.value}
            value={tab.value}
            className="relative cursor-pointer whitespace-nowrap px-4 pb-3 pt-1 text-sm font-medium text-muted transition-colors hover:text-fg data-[state=active]:text-fg"
          >
            {tab.label}
            {value === tab.value ? (
              <motion.span
                layoutId="tab-underline"
                className="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-accent"
              />
            ) : null}
          </TabsPrimitive.Trigger>
        ))}
      </TabsPrimitive.List>
      {children}
    </TabsPrimitive.Root>
  );
}

export const TabPanel = TabsPrimitive.Content;
