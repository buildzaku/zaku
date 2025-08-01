export class Path {
    #segments: string[];

    private constructor() {
        this.#segments = [];
    }

    static from(path: string): Path {
        const instance = new Path();
        instance.#segments = path === "" ? [] : path.split("/").filter(segment => segment !== "");

        return instance;
    }

    join(segment: string): Path {
        if (segment === "") return this;

        const instance = new Path();
        instance.#segments = [...this.#segments, segment];

        return instance;
    }

    parent(): Path | null {
        if (this.#segments.length === 0) {
            return null;
        }
        if (this.#segments.length === 1) {
            return Path.from("");
        }

        const instance = new Path();
        instance.#segments = this.#segments.slice(0, -1);

        return instance;
    }

    isEmpty(): boolean {
        return this.#segments.length === 0;
    }

    isChild(targetPath: string | Path): boolean {
        const targetSegments =
            typeof targetPath === "string"
                ? Path.from(targetPath).toSegments()
                : targetPath.toSegments();

        return (
            this.#segments.length <= targetSegments.length &&
            this.#segments.every((segment, index) => segment === targetSegments[index])
        );
    }

    toSegments(): string[] {
        return [...this.#segments];
    }

    toString(): string {
        return this.#segments.join("/");
    }
}
