import { createRouter, createWebHashHistory } from "vue-router";

const routes = [
  { path: "/", redirect: "/dictionary" },
  {
    path: "/dictionary",
    component: () => import("./views/DictionaryView.vue"),
    name: "Dictionary",
  },
];

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
});
