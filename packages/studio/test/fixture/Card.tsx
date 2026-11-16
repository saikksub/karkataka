export const schema = { title: { type: "string", title: "Title" } };
export default function Card(props: { title: string }) {
  return <h1>{props.title}</h1>;
}
