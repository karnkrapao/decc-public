import { CheckCircle2 } from "lucide-react";

export function AppToast({ message }: { message?: string }) {
  if (!message) return null;

  return (
    <div className="app-toast" role="status">
      <CheckCircle2 />
      <span>{message}</span>
    </div>
  );
}
