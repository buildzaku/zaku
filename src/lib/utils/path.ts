export class Path {
    #segments: string[];
    static readonly separator = "/";

    constructor(path?: string) {
        this.#segments = path ? Path.toSegments(path) : [];
    }

    static from(path: string): Path {
        return new Path(path);
    }

    static toSegments(path: string): string[] {
        return path === "" ? [] : path.split(Path.separator).filter(segment => segment !== "");
    }

    join(path: string): Path {
        if (path === "") return this;

        const newPath = new Path();
        newPath.#segments = [...this.#segments, ...Path.toSegments(path)];

        return newPath;
    }

    parent(): Path | null {
        if (this.#segments.length === 0) {
            return null;
        }
        if (this.#segments.length === 1) {
            return new Path("");
        }

        const parentPath = new Path();
        parentPath.#segments = this.#segments.slice(0, -1);

        return parentPath;
    }

    startsWith(prefix: Path): boolean {
        return (
            this.#segments.length >= prefix.#segments.length &&
            prefix.#segments.every((segment, index) => segment === this.#segments[index])
        );
    }

    isEmpty(): boolean {
        return this.#segments.length === 0;
    }

    toString(): string {
        return this.#segments.join(Path.separator);
    }
}
