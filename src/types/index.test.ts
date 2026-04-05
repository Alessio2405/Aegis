// Smoke tests — verify TypeScript types are self-consistent
import { describe, it, expect } from 'vitest';
import type { Memory, AuditRecord, Permission, AgentAnswer, Citation } from './index';

describe('Type smoke tests', () => {
  it('AgentAnswer shape is correct', () => {
    const answer: AgentAnswer = {
      answer: 'Test answer',
      citations: [],
      is_extractive: true,
      proposed_memories: [],
    };
    expect(answer.answer).toBe('Test answer');
    expect(answer.is_extractive).toBe(true);
  });

  it('Citation shape is correct', () => {
    const citation: Citation = {
      chunk_id: 'abc-123',
      doc_title: 'Project Alpha',
      snippet: 'First 240 chars of the chunk...',
    };
    expect(citation.chunk_id).toBe('abc-123');
  });

  it('Memory status enum values', () => {
    const statuses: Memory['status'][] = ['proposed', 'saved', 'rejected'];
    expect(statuses).toHaveLength(3);
  });

  it('AuditRecord outcome values', () => {
    const outcomes: AuditRecord['outcome'][] = ['allowed', 'denied'];
    expect(outcomes).toHaveLength(2);
  });

  it('Permission grant_type values', () => {
    const types: Permission['grant_type'][] = ['once', 'always'];
    expect(types).toHaveLength(2);
  });
});
