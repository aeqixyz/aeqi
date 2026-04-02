import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";

export default function InboxPage() {
  return (
    <div>
      <Header title="Inbox" />
      <EmptyState title="No pending items" description="Approvals and notifications will appear here." />
    </div>
  );
}
