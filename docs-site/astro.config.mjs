// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	integrations: [
		starlight({
			title: 'Redtrail',
			customCss: ['./src/styles/custom.css'],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/adriangs1996/redtrail' }],
			defaultLocale: 'en',
			locales: {
				en: { label: 'English', lang: 'en' },
				es: { label: 'Español', lang: 'es' },
			},
			sidebar: [
				{
					label: 'Getting Started',
					autogenerate: { directory: 'getting-started' },
				},
				{
					label: 'Core Concepts',
					autogenerate: { directory: 'core-concepts' },
				},
				{
					label: 'Guides',
					autogenerate: { directory: 'guides' },
				},
				{
					label: 'CLI Reference',
					autogenerate: { directory: 'cli-reference' },
				},
				{
					label: 'Skills',
					autogenerate: { directory: 'skills' },
				},
				{
					label: 'Configuration',
					autogenerate: { directory: 'configuration' },
				},
				{
					label: 'Contributing',
					autogenerate: { directory: 'contributing' },
				},
			],
		}),
	],
});
