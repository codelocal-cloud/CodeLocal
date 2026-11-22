import { ConstructionNotice } from "@/app/construction-notice";
import { PluginsHub } from "./plugins-hub";

export default function PluginsPage() {
  return (
    <>
      <ConstructionNotice />
      <PluginsHub />
    </>
  );
}
