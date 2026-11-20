import "@testing-library/jest-dom/vitest";

// jsdom has no canvas; the waveform simply renders nothing in tests.
HTMLCanvasElement.prototype.getContext = (() => null) as unknown as HTMLCanvasElement["getContext"];
