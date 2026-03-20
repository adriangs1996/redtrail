import { describe, it, expect } from 'vitest';
import { experimental_AstroContainer as AstroContainer } from 'astro/container';
import TerminalFrame from './TerminalFrame.astro';
import fs from 'node:fs';
import path from 'node:path';

const COMPONENT_SOURCE = fs.readFileSync(
  path.resolve(__dirname, './TerminalFrame.astro'),
  'utf-8',
);

async function render(props: Record<string, unknown> = {}, slotContent = '') {
  const container = await AstroContainer.create();
  return container.renderToString(TerminalFrame, {
    props,
    slots: { default: slotContent },
  });
}

describe('TerminalFrame', () => {
  describe('props', () => {
    it('renders with default props', async () => {
      const html = await render();
      expect(html).toContain('terminal-frame');
      expect(html).toContain('Terminal');
      expect(html).toContain('terminal-titlebar');
      expect(html).toContain('terminal-body');
    });

    it('renders custom title', async () => {
      const html = await render({ title: 'nmap scan' });
      expect(html).toContain('nmap scan');
    });

    it('renders custom prompt', async () => {
      const html = await render({ prompt: '# ' });
      expect(html).toContain('terminal-body');
    });

    it('renders with all props set', async () => {
      const html = await render(
        { title: 'Custom Title', prompt: '> ' },
        '<code>hello world</code>',
      );
      expect(html).toContain('Custom Title');
      expect(html).toContain('hello world');
    });
  });

  describe('structure', () => {
    it('renders traffic light dots', async () => {
      const html = await render();
      expect(html).toContain('dot-red');
      expect(html).toContain('dot-yellow');
      expect(html).toContain('dot-green');
    });

    it('renders titlebar and body sections', async () => {
      const html = await render();
      expect(html).toContain('terminal-titlebar');
      expect(html).toContain('terminal-body');
    });

    it('renders slot content in body', async () => {
      const html = await render({}, '<pre><code>$ whoami\nroot</code></pre>');
      expect(html).toContain('whoami');
      expect(html).toContain('root');
    });
  });

  describe('theme-aware styles (source verification)', () => {
    it('includes Mocha dark-mode colors in source', () => {
      expect(COMPONENT_SOURCE).toContain('#1e1e2e');
      expect(COMPONENT_SOURCE).toContain('#181825');
      expect(COMPONENT_SOURCE).toContain('#cdd6f4');
    });

    it('includes Latte light-mode colors in source', () => {
      expect(COMPONENT_SOURCE).toContain("data-theme='light'");
      expect(COMPONENT_SOURCE).toContain('#e6e9ef');
      expect(COMPONENT_SOURCE).toContain('#dce0e8');
      expect(COMPONENT_SOURCE).toContain('#4c4f69');
    });

    it('has Mocha dot colors in source', () => {
      expect(COMPONENT_SOURCE).toContain('#f38ba8');
      expect(COMPONENT_SOURCE).toContain('#f9e2af');
      expect(COMPONENT_SOURCE).toContain('#a6e3a1');
    });

    it('has Latte dot colors in source', () => {
      expect(COMPONENT_SOURCE).toContain('#d20f39');
      expect(COMPONENT_SOURCE).toContain('#df8e1d');
      expect(COMPONENT_SOURCE).toContain('#40a02b');
    });

    it('uses data-theme light selector for light mode', () => {
      const lightSelectors = COMPONENT_SOURCE.match(
        /:root\[data-theme='light'\]/g,
      );
      expect(lightSelectors).not.toBeNull();
      expect(lightSelectors!.length).toBeGreaterThanOrEqual(5);
    });
  });

  describe('edge cases', () => {
    it('renders with empty content', async () => {
      const html = await render({}, '');
      expect(html).toContain('terminal-body');
      expect(html).toContain('Terminal');
    });

    it('handles long output', async () => {
      const longLine = 'A'.repeat(500);
      const html = await render({}, `<code>${longLine}</code>`);
      expect(html).toContain(longLine);
    });

    it('handles multiline commands', async () => {
      const multiline = '$ nmap -sV \\\n  --script vuln \\\n  192.168.1.0/24';
      const html = await render({}, `<pre><code>${multiline}</code></pre>`);
      expect(html).toContain('nmap -sV');
      expect(html).toContain('--script vuln');
      expect(html).toContain('192.168.1.0/24');
    });

    it('source has overflow-x and pre-wrap for edge cases', () => {
      expect(COMPONENT_SOURCE).toContain('overflow-x: auto');
      expect(COMPONENT_SOURCE).toContain('pre-wrap');
    });

    it('renders with empty title', async () => {
      const html = await render({ title: '' });
      expect(html).toContain('terminal-title');
    });

    it('renders HTML content in slot', async () => {
      const html = await render(
        {},
        '<span style="color: green">success</span>',
      );
      expect(html).toContain('success');
    });
  });
});
