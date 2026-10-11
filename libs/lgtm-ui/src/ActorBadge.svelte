<script lang="ts">
  let { actor }: { actor: { name: string; kind: string } } = $props();
</script>

<span class="actor"
  ><b>{actor.name}</b><span
    class:agent={actor.kind === "agent"}
    class="actor-kind">{actor.kind === "agent" ? "agent" : "user"}</span
  ></span
>
