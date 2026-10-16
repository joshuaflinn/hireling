import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseCurriculum, schoolOf } from '../../src/lib/corpus/curriculum.js';

const mentalism = JSON.parse(
  readFileSync(join(import.meta.dirname, '../../../tests/fixtures/packs/classfeature-school-of-mentalism.json'), 'utf8'),
);

describe('curriculum from the Foundry school document', () => {
  it('reads every rank of School of Mentalism (captured pack doc)', () => {
    const c = parseCurriculum(mentalism.system.description.value);
    expect(c[0]).toEqual(['Daze', 'Figment']);
    expect(c[1]).toEqual(['Dizzying Colors', 'Sleep', 'Sure Strike']);
    expect(c[2]).toEqual(['Illusory Creature', 'Stupefy']);
  });

  it('returns {} for a feature with no curriculum', () => {
    expect(parseCurriculum('<p>No list here.</p>')).toEqual({});
  });

  it('reads the school out of Pathbuilder specials', () => {
    expect(schoolOf(['Arcane School: School of Mentalism', 'Arcane Bond'])).toBe('School of Mentalism');
    expect(schoolOf(['Shield Block'])).toBeNull();
  });
});
