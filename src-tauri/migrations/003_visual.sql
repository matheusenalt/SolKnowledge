DROP VIEW entry_search_content;
CREATE VIEW entry_search_content AS
SELECT e.id,e.title,e.full_name,
coalesce((SELECT group_concat(text,' ') FROM (
 SELECT CAST(atom AS TEXT) text FROM json_tree(e.payload_json)
 WHERE type='text' AND key NOT IN ('id','module_id','type','learning_status','options_coverage')
 UNION ALL SELECT CAST(j.atom AS TEXT) FROM blocks b,json_tree(b.payload_json) j
 WHERE b.entry_id=e.id AND j.type='text' AND j.key NOT IN ('id','type')
 UNION ALL SELECT a.filename FROM blocks b JOIN assets a ON a.id=json_extract(b.payload_json,'$.asset_id') WHERE b.entry_id=e.id
 UNION ALL SELECT a.filename FROM blocks b JOIN assets a ON a.id=json_extract(b.payload_json,'$.preview_asset_id') WHERE b.entry_id=e.id
)), '') body FROM entries e;
DELETE FROM entries_fts;
INSERT INTO entries_fts(entry_id,title,full_name,body) SELECT id,title,full_name,body FROM entry_search_content;
CREATE TRIGGER immutable_assets BEFORE UPDATE ON assets BEGIN SELECT RAISE(ABORT,'Asset IDs and content are immutable'); END;
