-- M6 在线词典：每部词典的选项（JSON）。
-- online：{"trigger":"always|localMiss|manual"（缺省 = 跟随全局设置）,"urlTemplate":"…{word}…","endpoint":"production|sandbox"}
ALTER TABLE dict ADD COLUMN options TEXT;
