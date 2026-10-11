import { animate, stagger } from "animejs";

export { stagger };

/** 入场动画：结束后把 anime 留在元素上的内联 transform/opacity 清掉。
 *  恒等 transform（matrix(1,0,0,1,0,0)）也会永久创建层叠上下文，把元素的
 *  z-index 弹层（变量面板、各类下拉）困在自己圈里，被 DOM 靠后的兄弟
 *  内容整层盖住——设置页魔法变量被「预设名称」穿透就是这个病。
 *  只包一次性入场；倾斜跟随、数字补间这类不落内联 transform 的别走这里。 */
export function entrance(targets, params) {
  const els =
    typeof targets === "string"
      ? [...document.querySelectorAll(targets)]
      : [...(targets ?? [])];
  const anim = animate(targets, params);
  anim.then(() => {
    for (const el of els) {
      el.style.transform = "";
      el.style.opacity = "";
    }
  });
  return anim;
}
