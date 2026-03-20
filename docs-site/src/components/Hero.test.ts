import { describe, it, expect } from 'vitest';
import { experimental_AstroContainer as AstroContainer } from 'astro/container';
import Hero from './Hero.astro';

const EN_PROPS = {
  locale: 'en',
  tagline: 'Pentesting orchestrator powered by deductive methodology.',
  quickStartLabel: 'Quick Start',
  quickStartDesc: 'Jump straight in. Install RT and run your first scan in minutes.',
  methodologyLabel: 'Understand the Methodology',
  methodologyDesc: 'Learn the deductive layers (L0–L4).',
  tutorialLabel: 'Hands-On Tutorial',
  tutorialDesc: 'Walk through a guided lab from recon to report.',
};

const ES_PROPS = {
  locale: 'es',
  tagline: 'Orquestador de pentesting con metodología deductiva.',
  quickStartLabel: 'Inicio Rápido',
  quickStartDesc: 'Ve directo al grano.',
  methodologyLabel: 'Entiende la Metodología',
  methodologyDesc: 'Aprende las capas deductivas (L0–L4).',
  tutorialLabel: 'Tutorial Práctico',
  tutorialDesc: 'Recorre un laboratorio guiado.',
};

async function render(props: Record<string, unknown>) {
  const container = await AstroContainer.create();
  return container.renderToString(Hero, { props });
}

describe('Hero', () => {
  describe('path cards', () => {
    it('renders three path cards with correct labels', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('Quick Start');
      expect(html).toContain('Understand the Methodology');
      expect(html).toContain('Hands-On Tutorial');
    });

    it('renders card descriptions', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain(EN_PROPS.quickStartDesc);
      expect(html).toContain(EN_PROPS.methodologyDesc);
      expect(html).toContain(EN_PROPS.tutorialDesc);
    });

    it('renders exactly three path-card links', async () => {
      const html = await render(EN_PROPS);
      const cardMatches = html.match(/<a[^>]*path-card/g);
      expect(cardMatches).toHaveLength(3);
    });
  });

  describe('locale-prefixed links', () => {
    it('points Quick Start to /getting-started/quickstart/', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('href="/getting-started/quickstart/"');
    });

    it('points Methodology to /core-concepts/overview/', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('href="/core-concepts/overview/"');
    });

    it('points Tutorial to /guides/simple-lab/', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('href="/guides/simple-lab/"');
    });

    it('uses /es/ prefix for Spanish locale', async () => {
      const html = await render(ES_PROPS);
      expect(html).toContain('href="/es/getting-started/quickstart/"');
      expect(html).toContain('href="/es/core-concepts/overview/"');
      expect(html).toContain('href="/es/guides/simple-lab/"');
    });
  });

  describe('English context', () => {
    it('renders English tagline', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain(EN_PROPS.tagline);
    });

    it('renders English card labels', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('Quick Start');
      expect(html).toContain('Understand the Methodology');
      expect(html).toContain('Hands-On Tutorial');
    });

    it('renders redtrail logo text', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('hero-logo');
      expect(html).toContain('redtrail');
    });
  });

  describe('Spanish context', () => {
    it('renders Spanish tagline', async () => {
      const html = await render(ES_PROPS);
      expect(html).toContain(ES_PROPS.tagline);
    });

    it('renders Spanish card labels', async () => {
      const html = await render(ES_PROPS);
      expect(html).toContain('Inicio Rápido');
      expect(html).toContain('Entiende la Metodología');
      expect(html).toContain('Tutorial Práctico');
    });

    it('renders Spanish card descriptions', async () => {
      const html = await render(ES_PROPS);
      expect(html).toContain(ES_PROPS.quickStartDesc);
      expect(html).toContain(ES_PROPS.methodologyDesc);
      expect(html).toContain(ES_PROPS.tutorialDesc);
    });
  });

  describe('structure', () => {
    it('renders hero section wrapper', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('hero-inner');
    });

    it('embeds a TerminalFrame', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('hero-terminal');
      expect(html).toContain('terminal-frame');
    });

    it('terminal shows RT session snippet', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('rt scan');
      expect(html).toContain('rt suggest');
    });

    it('renders card icons', async () => {
      const html = await render(EN_PROPS);
      expect(html).toContain('card-icon');
    });
  });
});
