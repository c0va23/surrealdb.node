napi build --platform --release $@
echo -e "import {\n\tConnectionOptions,\n\tsurrealdbNodeEngines as embeddedSurrealdbNodeEngines,\n} from \"./lib-src/embedded.js\";\n$(cat index.d.ts)" > index.d.ts
echo -e '\nexport declare const surrealdbNodeEngines: typeof embeddedSurrealdbNodeEngines' >> index.d.ts
mkdir -p artifacts
mv surrealdb.node.*.node artifacts/
node move_artifacts.js
