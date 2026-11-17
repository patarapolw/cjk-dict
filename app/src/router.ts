import { createRouter, createWebHashHistory } from "vue-router";

const routes = [
  { path: "/", redirect: "/dictionary" },
  {
    path: "/dictionary",
    component: () => import("./views/DictionaryView.vue"),
    name: "Dictionary",
  },
  {
    path: "/kanji",
    component: () => import("./views/CharacterView.vue"),
    name: "Characters",
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
