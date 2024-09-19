<script lang="ts">
    export let size: number;
    export let fill = "currentColor";
</script>

<svg
    width={`${size}px`}
    height={`${size}px`}
    fill="none"
    class={$$props["class"]}
    viewBox="0 0 24 24"
    xmlns="http://www.w3.org/2000/svg"
    stroke-width="1.75"
    stroke-linecap="round"
    stroke-linejoin="round"
>
    <path d="M16 14V22" stroke={fill} />
    <path d="M12 18H20" stroke={fill} />
    <path
        d="M10 21H6C5.46957 21 4.96086 20.8209 4.58579 20.5021C4.21071 20.1833 4 19.7509 4 19.3V5.7C4 5.24913 4.21071 4.81673 4.58579 4.49792C4.96086 4.17911 5.46957 4 6 4H14.5L20 9.5V13"
        stroke={fill}
    />
    <path
        d="M14 4V8C14 8.53043 14.2107 9.03914 14.5858 9.41421C14.9609 9.78929 15.4696 10 16 10H20"
        stroke={fill}
    />
</svg>
