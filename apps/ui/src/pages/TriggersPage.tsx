import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";

export default function TriggersPage() {
  return (
    <div>
      <Header title="Triggers" />
      <EmptyState title="No triggers" description="Automated trigger management will appear here." />
    </div>
  );
}
