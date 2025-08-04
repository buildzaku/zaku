<script lang="ts">
  import type { IconProps } from ".";

  let { size, fill = "currentColor", class: className }: IconProps = $props();
</script>

<svg
  width={`${size}px`}
  height={`${size}px`}
  fill="none"
  class={className}
  viewBox="0 0 24 24"
  xmlns="http://www.w3.org/2000/svg"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path
    d="M10.7 21.0001H4C3.46957 21.0001 2.96086 20.7894 2.58579 20.4143C2.21071 20.0392 2 19.5305 2 19.0001V6.0001C2 5.46966 2.21071 4.96096 2.58579 4.58588C2.96086 4.21081 3.46957 4.0001 4 4.0001H7.9C8.23449 3.99682 8.56445 4.07748 8.8597 4.23472C9.15495 4.39195 9.40604 4.62072 9.59 4.9001L10.4 6.1001C10.5821 6.37663 10.83 6.60362 11.1215 6.7607C11.413 6.91778 11.7389 7.00004 12.07 7.0001H20C20.5304 7.0001 21.0391 7.21081 21.4142 7.58588C21.7893 7.96096 22 8.46966 22 9.0001V13.1001"
    stroke={fill}
  />
  <path d="M17 14.0001V22.0001" stroke={fill} />
  <path d="M13 18.0001H21" stroke={fill} />
</svg>
