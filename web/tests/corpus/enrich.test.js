// Foundry enricher (specs/015): the pack's markup becomes readable text
// the way the books print it, and cross-references keep their targets.
import { describe, it, expect } from 'vitest';
import { enrich, linkedIds } from '../../src/lib/corpus/enrich.js';

describe('enrich', () => {
  it('turns a UUID link into an xref anchor carrying kind + upstream id', () => {
    const out = enrich('The target is @UUID[Compendium.pf2e.conditionitems.Item.TBSHQspnbcqxsmjL]{Frightened 1}.');
    expect(out).toBe(
      'The target is <a class="xref" data-uuid-kind="condition" data-uuid-id="TBSHQspnbcqxsmjL">Frightened 1</a>.',
    );
  });

  it('renders damage with nested brackets as dice + type', () => {
    expect(enrich('@Damage[2d6[fire]]')).toBe('<b class="roll">2d6 fire</b>');
    expect(enrich('@Damage[(2*@actor.level)[healing]]')).toBe('<b class="roll">2*level healing</b>');
  });

  it('renders checks the way the book phrases them', () => {
    expect(enrich('@Check[reflex|against:class-spell|basic]')).toBe('<b class="check">basic Reflex (vs spell DC)</b>');
    expect(enrich('@Check[flat|dc:5]')).toBe('<b class="check">DC 5 flat check</b>');
  });

  it('renders templates, inline rolls and action links', () => {
    expect(enrich('@Template[cone|distance:30]')).toBe('<b class="area">30-foot cone</b>');
    expect(enrich('[[/r 1d4 #rounds]]{1d4 rounds}')).toBe('<b class="roll">1d4 rounds</b>');
    expect(enrich('[[/act seek]]')).toBe('<a class="xref" data-act="seek">Seek</a>');
  });

  it('prefers an explicit {label} and escapes it', () => {
    expect(enrich('@Check[will]{a "save"}')).toBe('<b class="check">a &quot;save&quot;</b>');
  });

  it('leaves unknown compendia as dead text, never a guessed link', () => {
    expect(enrich('@UUID[Compendium.pf2e.bestiary-ability-glossary-srd.Item.abc]{Darkvision}')).toBe(
      '<span class="xref-dead">Darkvision</span>',
    );
  });
});

describe('linkedIds', () => {
  it('reads a school curriculum out of its description links', () => {
    const html =
      'cantrips: @UUID[Compendium.pf2e.spells-srd.Item.4gBIw4IDrSfFHik4]{Daze}, ' +
      '@UUID[Compendium.pf2e.spells-srd.Item.0zU8CPejjQFnhZFI]{Figment} ' +
      '@UUID[Compendium.pf2e.conditionitems.Item.x]{Frightened}';
    expect(linkedIds(html, 'spell')).toEqual([
      { kind: 'spell', id: '4gBIw4IDrSfFHik4', label: 'Daze' },
      { kind: 'spell', id: '0zU8CPejjQFnhZFI', label: 'Figment' },
    ]);
  });
});
