import { Link } from "@tanstack/react-router";
import { Ghost } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button, EmptyState, Page } from "../components/ui";

export function NotFoundPage() {
  const { t } = useTranslation();
  return (
    <Page className="px-4 pt-16">
      <EmptyState icon={<Ghost className="size-8" />} title={t("notFound.title")}>
        <p>{t("notFound.body")}</p>
        <Link to="/">
          <Button variant="primary" className="mt-5">
            {t("notFound.home")}
          </Button>
        </Link>
      </EmptyState>
    </Page>
  );
}
