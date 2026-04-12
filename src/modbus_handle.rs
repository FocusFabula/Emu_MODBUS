use crate::device::Device;
use crate::device::ModbusDevice;
use crate::request::RequestFunction;

pub enum ModbusException {
    IllegalFunction,
    IllegalDataAdress,
    IllegalDataValue,
    ServerDeviceFailure,
    Acknowledge,
    ServerDeviceBusy,
    MemoryParityError,
    GatewayPathUnavailable,
    GatewayTargetDeviceFailedToRespond,
}
