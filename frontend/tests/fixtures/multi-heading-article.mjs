export const multiHeadingArticle = {
  title: "目录跟随测试：一篇有很多标题的文章",
  content: `## 出发之前

这是一段足够长的开场，用来确认目录从文章的第一个二级标题开始。

### 准备开发环境

三级标题应该在目录中缩进显示。

#### 检查 Docker

四级标题也应该保留，并且拥有更深一级的缩进。

## 第一次实现

滚动到这里时，右侧目录应该把“第一次实现”标记为当前位置。

### 处理重复标题

重复标题必须得到不同的链接地址。

## 第一次实现

这是一个故意保留的重复标题，用来验证目录锚点不会冲突。

## 收尾与验证

滚动到文章末尾时，最后一个目录项应该保持高亮。`,
};

export const expectedTocItems = [
  { id: "article-出发之前", text: "出发之前", level: 2 },
  { id: "article-准备开发环境", text: "准备开发环境", level: 3 },
  { id: "article-检查-docker", text: "检查 Docker", level: 4 },
  { id: "article-第一次实现", text: "第一次实现", level: 2 },
  { id: "article-处理重复标题", text: "处理重复标题", level: 3 },
  { id: "article-第一次实现-2", text: "第一次实现", level: 2 },
  { id: "article-收尾与验证", text: "收尾与验证", level: 2 },
];
