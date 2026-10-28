-- HopsDB demo tables: table & column comments (Chinese)
-- Each line is one statement executed by the seed script via sqlplus.
-- Run the seed with NLS_LANG=AL32UTF8 so Chinese is stored correctly.

comment on table HOPS_CUSTOMERS    is '客户';
comment on table HOPS_PRODUCTS     is '商品';
comment on table HOPS_ORDERS       is '订单';
comment on table HOPS_ORDER_ITEMS  is '订单明细';
comment on table HOPS_NOTES        is '备注';
comment on table HOPS_EVENTS       is '事件';
comment on table HOPS_CODES        is '编码字典';
comment on table HOPS_TEXTS        is '文本';
comment on table HOPS_MESSAGES     is '消息';
comment on table HOPS_BIG_LOGS     is '大日志记录';

comment on column HOPS_CUSTOMERS.CUSTOMER_ID    is '客户ID';
comment on column HOPS_CUSTOMERS.CUSTOMER_CODE  is '客户编码';
comment on column HOPS_CUSTOMERS.CUSTOMER_NAME  is '客户姓名';
comment on column HOPS_CUSTOMERS.COUNTRY        is '国家';
comment on column HOPS_CUSTOMERS.CITY           is '城市';
comment on column HOPS_CUSTOMERS.STATUS         is '状态';
comment on column HOPS_CUSTOMERS.CREATED_AT     is '创建时间';
comment on column HOPS_CUSTOMERS.REMARK         is '备注';

comment on column HOPS_PRODUCTS.PRODUCT_ID    is '商品ID';
comment on column HOPS_PRODUCTS.PRODUCT_CODE  is '商品编码';
comment on column HOPS_PRODUCTS.PRODUCT_NAME  is '商品名称';
comment on column HOPS_PRODUCTS.CATEGORY      is '分类';
comment on column HOPS_PRODUCTS.PRICE         is '价格';
comment on column HOPS_PRODUCTS.STOCK_QTY     is '库存数量';
comment on column HOPS_PRODUCTS.TAG           is '标签';
comment on column HOPS_PRODUCTS.NOTE          is '说明';

comment on column HOPS_ORDERS.ORDER_ID       is '订单ID';
comment on column HOPS_ORDERS.CUSTOMER_ID    is '客户ID';
comment on column HOPS_ORDERS.ORDER_NO       is '订单号';
comment on column HOPS_ORDERS.ORDER_STATUS   is '订单状态';
comment on column HOPS_ORDERS.ORDER_DATE     is '下单时间';
comment on column HOPS_ORDERS.PAY_METHOD     is '支付方式';
comment on column HOPS_ORDERS.TOTAL_AMOUNT   is '总金额';
comment on column HOPS_ORDERS.REMARK         is '备注';
comment on column HOPS_ORDERS.UPDATED_AT     is '更新时间';

comment on column HOPS_ORDER_ITEMS.ITEM_ID     is '明细ID';
comment on column HOPS_ORDER_ITEMS.ORDER_ID    is '订单ID';
comment on column HOPS_ORDER_ITEMS.PRODUCT_ID  is '商品ID';
comment on column HOPS_ORDER_ITEMS.QTY         is '数量';
comment on column HOPS_ORDER_ITEMS.UNIT_PRICE  is '单价';
comment on column HOPS_ORDER_ITEMS.LINE_TOTAL  is '行金额';
comment on column HOPS_ORDER_ITEMS.ITEM_NOTE   is '明细备注';
comment on column HOPS_ORDER_ITEMS.CREATED_AT  is '创建时间';

comment on column HOPS_NOTES.NOTE_ID     is '备注ID';
comment on column HOPS_NOTES.NOTE_TITLE  is '标题';
comment on column HOPS_NOTES.NOTE_BODY   is '正文';
comment on column HOPS_NOTES.LANG_TAG    is '语言';
comment on column HOPS_NOTES.LEVEL_TAG   is '级别';
comment on column HOPS_NOTES.CREATED_AT  is '创建时间';

comment on column HOPS_EVENTS.EVENT_ID       is '事件ID';
comment on column HOPS_EVENTS.EVENT_TYPE     is '事件类型';
comment on column HOPS_EVENTS.EVENT_NAME     is '事件名称';
comment on column HOPS_EVENTS.EVENT_SOURCE   is '来源';
comment on column HOPS_EVENTS.EVENT_STATUS   is '状态';
comment on column HOPS_EVENTS.OCCURRED_AT    is '发生时间';
comment on column HOPS_EVENTS.DETAIL_TEXT    is '详情';
comment on column HOPS_EVENTS.EXTRA_TEXT     is '扩展信息';

comment on column HOPS_CODES.CODE_ID      is '编码ID';
comment on column HOPS_CODES.CODE_GROUP   is '编码组';
comment on column HOPS_CODES.CODE_KEY     is '编码键';
comment on column HOPS_CODES.CODE_VALUE   is '编码值';
comment on column HOPS_CODES.SORT_NO      is '排序';
comment on column HOPS_CODES.ACTIVE_FLAG  is '启用标记';

comment on column HOPS_TEXTS.TEXT_ID     is '文本ID';
comment on column HOPS_TEXTS.TITLE       is '标题';
comment on column HOPS_TEXTS.BODY_TEXT   is '正文';
comment on column HOPS_TEXTS.MARKER      is '标记';
comment on column HOPS_TEXTS.LOCALE_TAG  is '语言';
comment on column HOPS_TEXTS.CREATED_AT  is '创建时间';
comment on column HOPS_TEXTS.UPDATED_AT  is '更新时间';

comment on column HOPS_MESSAGES.MSG_ID        is '消息ID';
comment on column HOPS_MESSAGES.USER_NAME     is '用户名';
comment on column HOPS_MESSAGES.SUBJECT       is '主题';
comment on column HOPS_MESSAGES.MESSAGE_BODY  is '正文';
comment on column HOPS_MESSAGES.LANG_TAG      is '语言';
comment on column HOPS_MESSAGES.SENT_AT       is '发送时间';
comment on column HOPS_MESSAGES.STATUS_TAG    is '状态';
comment on column HOPS_MESSAGES.SYMBOLS       is '符号';

comment on column HOPS_BIG_LOGS.LOG_ID        is '日志ID';
comment on column HOPS_BIG_LOGS.CUSTOMER_ID   is '客户ID';
comment on column HOPS_BIG_LOGS.EVENT_KIND    is '事件类别';
comment on column HOPS_BIG_LOGS.CHANNEL_TAG   is '渠道';
comment on column HOPS_BIG_LOGS.AMOUNT        is '金额';
comment on column HOPS_BIG_LOGS.NOTE_TEXT     is '说明';
comment on column HOPS_BIG_LOGS.JP_TEXT       is '日文文本';
comment on column HOPS_BIG_LOGS.CN_TEXT       is '中文文本';
comment on column HOPS_BIG_LOGS.CREATED_AT    is '创建时间';