import Header from "@/components/Header";
import EmptyState from "@/components/EmptyState";

export default function DepartmentsPage() {
  return (
    <div>
      <Header title="Departments" />
      <EmptyState title="No departments" description="Department organization chart will appear here." />
    </div>
  );
}
