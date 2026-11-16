declare module "virtual:karka-project" {
  const project: import("./types.ts").PlayerProject | null;
  export default project;
}
declare module "virtual:karka-components" {
  const components: Record<string, import("./types.ts").ComponentModule>;
  export default components;
}
