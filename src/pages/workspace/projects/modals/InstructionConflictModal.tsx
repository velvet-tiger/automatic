import type { InstructionFileConflict } from "../types";
import { ContentConflictModal } from "./ContentConflictModal";

interface InstructionConflictModalProps {
  conflict: InstructionFileConflict;
  projectName: string;
  onAdopt: (adoptedContent: string) => void;
  onOverwrite: () => void;
  onClose: () => void;
}

export function InstructionConflictModal({
  conflict,
  projectName: _projectName,
  onAdopt,
  onOverwrite,
  onClose,
}: InstructionConflictModalProps) {
  return (
    <ContentConflictModal
      kindLabel="Instruction File Conflict"
      subject={conflict.filename}
      diskContent={conflict.disk_content}
      automaticContent={conflict.automatic_content}
      onAdopt={onAdopt}
      onOverwrite={onOverwrite}
      onClose={onClose}
      overwriteDescriptionEmpty="Discard external changes. Only configured rules will remain."
      overwriteDescription="Replace the on-disk file with Automatic's editor content."
    />
  );
}
