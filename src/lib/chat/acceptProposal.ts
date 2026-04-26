import { fileDelete, fileMoveWithRefs, fileWrite } from '$lib/ipc/file';
import type { ProposalPayload } from './toolCallViewModel';

export interface ProposalExecutionOutcome {
  resultText: string;
  nextOpenPath: string | null;
}

export async function performProposalAction(
  proposal: ProposalPayload,
  currentFilePath: string | null
): Promise<ProposalExecutionOutcome> {
  switch (proposal.proposal_kind) {
    case 'summary':
    case 'tag_update':
    case 'moc':
    case 'note_edit': {
      await fileWrite(proposal.target_rel_path, proposal.proposed_content);
      return {
        resultText: proposal.proposal_kind === 'moc' ? 'created' : 'written',
        nextOpenPath: proposal.target_rel_path
      };
    }
    case 'rename_note': {
      const sourceRelPath = readString(proposal.metadata, 'source_rel_path');
      const newRelPath = readString(proposal.metadata, 'new_rel_path') ?? proposal.target_rel_path;
      if (!sourceRelPath) {
        throw new Error('rename proposal 缺少 source_rel_path');
      }
      await fileMoveWithRefs(sourceRelPath, newRelPath);
      return {
        resultText: 'renamed',
        nextOpenPath: newRelPath
      };
    }
    case 'delete_note': {
      await fileDelete(proposal.target_rel_path);
      return {
        resultText: 'deleted',
        nextOpenPath: currentFilePath === proposal.target_rel_path ? null : currentFilePath
      };
    }
    default:
      throw new Error(`unsupported proposal kind: ${(proposal as ProposalPayload).proposal_kind}`);
  }
}

function readString(
  value: Record<string, unknown> | undefined,
  key: string
): string | null {
  const candidate = value?.[key];
  return typeof candidate === 'string' && candidate.trim() ? candidate.trim() : null;
}
