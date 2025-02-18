<script lang="ts">
    import type { IconProps } from ".";

    let { size, fill = "currentColor", class: className }: IconProps = $props();
</script>

<svg
    width={`${size}px`}
    height={`${size}px`}
    {fill}
    class={className}
    viewBox="0 0 12 12"
    xmlns="http://www.w3.org/2000/svg"
>
    <circle cx="6" cy="6" r="6" />
</svg>
