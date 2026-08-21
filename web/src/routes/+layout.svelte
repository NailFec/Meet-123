<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>Meet-123 中转</title>
	<link rel="icon" href={favicon} />
</svelte:head>

{@render children()}
