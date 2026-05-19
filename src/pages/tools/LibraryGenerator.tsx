import { Sparkles } from "lucide-react";
import LibraryGeneratorPanel from "../../components/LibraryGeneratorPanel";

export default function LibraryGenerator() {
  return (
    <div className="flex-1 h-full overflow-y-auto p-8 custom-scrollbar bg-bg-base">
      <div className="max-w-3xl mx-auto space-y-8">
        <div className="flex items-start gap-4">
          <div className="p-3 rounded-xl bg-brand/10 border border-brand/20 shrink-0">
            <Sparkles size={20} className="text-brand" />
          </div>
          <div>
            <h1 className="text-2xl font-semibold text-text-base mb-2">Library Generator</h1>
            <p className="text-text-muted text-[13px] leading-relaxed max-w-2xl">
              Generate a new skill, command, rule, or sub-agent from a short description.
              Review the result, request changes, and save it to your library when ready.
            </p>
          </div>
        </div>

        <LibraryGeneratorPanel />
      </div>
    </div>
  );
}
