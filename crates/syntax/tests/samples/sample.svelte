<script lang="ts">
  // A greeting, twice.
  export let name: string = "world";
  let times = 2;
</script>

<h1>Hello, {name}!</h1>
{#if times > 1}
  <p>Hello again, {name.toUpperCase()}!</p>
{/if}

<style>
  h1 { color: teal; }
</style>
