import type { CustomAssetConflict, CustomAssetKind } from "../types";
import { ContentConflictModal } from "./ContentConflictModal";

interface CustomAssetConflictModalProps {
  conflict: CustomAssetConflict;
  onAdopt: () => void;
  onOverwrite: () => void;
  onClose: () => void;
}

function kindLabel(kind: CustomAssetKind): string {
  switch (kind) {
    case "skill":
      return "Project Skill Conflict";
    case "rule":
      return "Project Rule Conflict";
    case "agent":
      return "Project Agent Conflict";
    case "command":
      return "Project Command Conflict";
  }
}

function kindNoun(kind: CustomAssetKind): string {
  switch (kind) {
    case "skill":
      return "skill";
    case "rule":
      return "rule";
    case "agent":
      return "agent";
    case "command":
      return "command";
  }
}

export function CustomAssetConflictModal({
  conflict,
  onAdopt,
  onOverwrite,
  onClose,
}: CustomAssetConflictModalProps) {
  const noun = kindNoun(conflict.kind);
  return (
    <ContentConflictModal
      kindLabel={kindLabel(conflict.kind)}
      subject={conflict.name}
      diskContent={conflict.disk_content}
      automaticContent={conflict.automatic_content}
      onAdopt={() => onAdopt()}
      onOverwrite={onOverwrite}
      onClose={onClose}
      modifiedMessage={`differs from the version stored in Automatic (${conflict.path}).`}
      adoptTitle={`Use on-disk ${noun}`}
      adoptDescription={`Keep the on-disk file and update Automatic's stored project ${noun} to match.`}
      overwriteTitle="Overwrite with Automatic content"
      overwriteDescription={`Replace the on-disk file with Automatic's stored project ${noun}.`}
      overwriteDescriptionEmpty={`Discard on-disk changes. Automatic's stored content will be written.`}
    />
  );
}
