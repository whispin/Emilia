-- PostgreSQL Schema for Proxy Database
-- 代理服务器信息表

CREATE TABLE IF NOT EXISTS proxies (
    id BIGSERIAL PRIMARY KEY,
    ip INET NOT NULL,
    port INTEGER NOT NULL,
    country_code VARCHAR(2),
    country_name VARCHAR(100),
    city_code VARCHAR(100),
    city_name VARCHAR(100),
    asn_number VARCHAR(20),
    org_name VARCHAR(255),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- 唯一约束：同一个IP和端口组合只能有一条记录
    CONSTRAINT unique_proxy UNIQUE (ip, port)
);

-- 创建索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_proxies_ip ON proxies(ip);
CREATE INDEX IF NOT EXISTS idx_proxies_updated_at ON proxies(updated_at);
CREATE INDEX IF NOT EXISTS idx_proxies_country_code ON proxies(country_code);
CREATE INDEX IF NOT EXISTS idx_proxies_asn_number ON proxies(asn_number);

-- 创建注释
COMMENT ON TABLE proxies IS '存储可用代理服务器信息';
COMMENT ON COLUMN proxies.ip IS '代理服务器 IP 地址';
COMMENT ON COLUMN proxies.port IS '代理服务器端口';
COMMENT ON COLUMN proxies.country_code IS '国家代码（ISO 3166-1 alpha-2）';
COMMENT ON COLUMN proxies.country_name IS '国家中文名称';
COMMENT ON COLUMN proxies.city_code IS '城市代码（英文城市名）';
COMMENT ON COLUMN proxies.city_name IS '城市中文名称';
COMMENT ON COLUMN proxies.asn_number IS '自治系统编号';
COMMENT ON COLUMN proxies.org_name IS 'ISP/组织名称';
COMMENT ON COLUMN proxies.updated_at IS '最后更新时间（用于数据清理）';
COMMENT ON COLUMN proxies.created_at IS '首次创建时间';

-- 示例查询：获取最新更新的代理列表
-- SELECT * FROM proxies ORDER BY updated_at DESC LIMIT 100;

-- 示例查询：按国家统计代理数量
-- SELECT country_name, COUNT(*) as count FROM proxies GROUP BY country_name ORDER BY count DESC;

-- 示例查询：清理旧数据（保留最新一次更新的数据）
-- DELETE FROM proxies WHERE updated_at < (SELECT MAX(updated_at) FROM proxies);